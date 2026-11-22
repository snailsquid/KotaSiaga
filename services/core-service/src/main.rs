//! KotaSiaga core-service: the five business bounded contexts as modules in one process.
//!
//! Layout rules (see AGENTS.md):
//! - Each context owns its own tables and talks to other contexts only through domain events.
//! - External APIs (Open-Meteo, PetaBencana, OSRM, Tourism, Health) are called only from `adapters`.
//! - Hot read data lives in memory and is recomputed on domain events, never per request.

mod adapters;
mod alerts;
mod config;
mod db;
mod engine;
mod feed;
mod http;
mod mock;
mod reports;
mod risk;
mod routing;
mod rules;
mod store;
mod verification;

use std::net::SocketAddr;

use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub config: std::sync::Arc<config::Config>,
    pub db: sqlx::SqlitePool,
    pub engine: std::sync::Arc<engine::Engine>,
    pub partners: std::sync::Arc<adapters::partners::Partners>,
    pub routes: std::sync::Arc<adapters::osrm::RouteSource>,
}

/// Builds the adapters from config (no network calls here).
pub fn adapters_for(
    config: &config::Config,
) -> (
    adapters::http::Upstream,
    std::sync::Arc<adapters::partners::Partners>,
    std::sync::Arc<adapters::osrm::RouteSource>,
) {
    use adapters::partners::{Partner, Partners};
    let u = &config.upstreams;
    let up = adapters::http::Upstream::new(u.timeout_ms);
    let partners = Partners::new(
        Partner {
            base_url: u.tourism_base_url.clone(),
            path: "/feed/destinations".into(),
            api_key: u.tourism_api_key.clone(),
            mock: config.mock.upstream_tourism || u.tourism_base_url.is_none(),
        },
        Partner {
            base_url: u.health_base_url.clone(),
            path: u.health_events_path.clone(),
            api_key: u.health_api_key.clone(),
            mock: config.mock.upstream_health || u.health_base_url.is_none(),
        },
    );
    let live_osrm = !config.mock.enabled || u.osrm_in_mock;
    let routes = adapters::osrm::RouteSource::new(up.clone(), u.osrm_url.clone(), live_osrm);
    (
        up,
        std::sync::Arc::new(partners),
        std::sync::Arc::new(routes),
    )
}

#[tokio::main]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("backup") {
        backup().await;
        return;
    }
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        // Plain text in containers so `docker compose logs` stays greppable (call-log evidence).
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .init();

    let config = config::Config::from_env()
        .unwrap_or_else(|e| fatal(2, &format!("invalid configuration: {e}")));
    if config.mock.enabled {
        tracing::info!(
            seed = config.mock.seed,
            tick_seconds = config.mock.tick_seconds,
            time_scale = config.mock.time_scale,
            "MOCK_MODE enabled"
        );
    }

    let db = db::open(&config.database_path).await.unwrap_or_else(|e| {
        fatal(
            1,
            &format!(
                "cannot open database {}: {e} (does its directory exist and is it writable by this user?)",
                config.database_path
            ),
        )
    });

    if config.api_keys.is_empty() && config.admin_api_key.is_none() {
        tracing::warn!("no API_KEYS or ADMIN_API_KEY set: every keyed endpoint will return 401");
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let engine = std::sync::Arc::new(engine::Engine::new(&config, chrono::Utc::now()));
    let loaded = store::load(&db, chrono::Utc::now())
        .await
        .unwrap_or_else(|e| fatal(1, &format!("cannot read stored data: {e}")));
    tracing::info!(
        live_reports = loaded.live.len(),
        subscriptions = loaded.subscriptions.len(),
        "stored data loaded"
    );
    engine.attach_store(loaded, store::spawn_writer(db.clone()), chrono::Utc::now());
    if config.jwt_secret.is_none() {
        tracing::warn!(
            "JWT_SECRET not set (or shorter than 32 bytes): every user endpoint returns 401"
        );
    }
    let tick = std::time::Duration::from_secs(if config.mock.enabled {
        config.mock.tick_seconds
    } else {
        5
    });
    spawn_ticker(engine.clone(), tick);
    let (up, partners, routes) = adapters_for(&config);
    if !config.mock.enabled {
        adapters::feeds::spawn_live_inputs(engine.clone(), up.clone(), &config.upstreams);
    }
    adapters::feeds::spawn_partner_feeds(partners.clone(), up, &config.upstreams);
    tracing::info!(
        tourism = if partners.tourism_cfg.mock {
            "mock"
        } else {
            "live"
        },
        health = if partners.health_cfg.mock {
            "mock"
        } else {
            "live"
        },
        osrm = if !config.mock.enabled || config.upstreams.osrm_in_mock {
            "live"
        } else {
            "mock"
        },
        "upstream adapters ready"
    );
    let state = AppState {
        config: std::sync::Arc::new(config),
        db,
        engine,
        partners,
        routes,
    };
    let app = http::router(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| fatal(1, &format!("cannot bind {addr}: {e}")));
    tracing::info!(%addr, "core-service listening");
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        fatal(1, &format!("server error: {e}"));
    }
}

/// Logs a startup failure straight to stderr (not through tracing, so RUST_LOG cannot hide it)
/// and exits. The runtime image is distroless with no shell, so this line is the only clue.
fn fatal(code: i32, msg: &str) -> ! {
    eprintln!("core-service: fatal: {msg}");
    std::process::exit(code);
}

/// Drives time-based rules (expiry, decay) and, in mock mode, the generator.
fn spawn_ticker(engine: std::sync::Arc<engine::Engine>, every: std::time::Duration) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(every);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            interval.tick().await;
            let e = engine.clone();
            // The tick is CPU work; keep it off the request-serving threads.
            let _ = tokio::task::spawn_blocking(move || e.tick(chrono::Utc::now())).await;
        }
    });
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}

/// `core-service backup [dest]`: consistent online copy with `VACUUM INTO` (safe in WAL mode while the
/// service runs). Works in the distroless image: `docker compose exec core-service /app backup`.
/// Default destination `/data/backup.db` (replaced each time); prints the path and the result
/// of `PRAGMA integrity_check` on the copy.
async fn backup() {
    let src = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "/data/core.db".into());
    let dest = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "/data/backup.db".into());
    let _ = std::fs::remove_file(&dest);
    let db = db::open(&src)
        .await
        .unwrap_or_else(|e| fatal_backup(&format!("cannot open {src}: {e}")));
    sqlx::query("VACUUM INTO ?")
        .bind(&dest)
        .execute(&db)
        .await
        .unwrap_or_else(|e| fatal_backup(&format!("VACUUM INTO failed: {e}")));
    db.close().await;
    let copy = db::open(&dest)
        .await
        .unwrap_or_else(|e| fatal_backup(&format!("cannot open the copy: {e}")));
    let check: (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(&copy)
        .await
        .unwrap_or_else(|e| fatal_backup(&format!("integrity check failed: {e}")));
    copy.close().await;
    println!("backup {dest} integrity={}", check.0);
    if check.0 != "ok" {
        std::process::exit(1);
    }
}

fn fatal_backup(msg: &str) -> ! {
    eprintln!("core-service: backup failed: {msg}");
    std::process::exit(1);
}

#[cfg(test)]
mod contract_copy {
    /// The gateway serves `gateway/docs/openapi.yaml`; it must equal the canonical contract.
    #[test]
    fn gateway_docs_spec_matches_repo_root() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        let canonical = std::fs::read_to_string(format!("{root}/openapi.yaml")).unwrap();
        let copy = std::fs::read_to_string(format!("{root}/gateway/docs/openapi.yaml")).unwrap();
        assert!(
            canonical == copy,
            "gateway/docs/openapi.yaml is out of date: run scripts/sync_openapi.sh"
        );
    }
}
