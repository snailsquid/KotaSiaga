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
    tune_malloc();
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
    let ticker = spawn_ticker(engine.clone(), tick);
    let (up, partners, routes) = adapters_for(&config);
    if !config.mock.enabled {
        adapters::feeds::spawn_live_inputs(engine.clone(), up.clone(), &config.upstreams);
    }
    adapters::feeds::spawn_partner_feeds(partners.clone(), engine.clone(), up, &config.upstreams);
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
        engine: engine.clone(),
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
    // SIGTERM (every redeploy): stop producing changes, then wait until everything queued is in
    // SQLite before the process exits.
    ticker.abort();
    match tokio::time::timeout(std::time::Duration::from_secs(10), engine.flush()).await {
        Ok(Ok(())) => tracing::info!("pending writes flushed; shutting down"),
        Ok(Err(e)) => tracing::error!(error = e, "flush on shutdown failed"),
        Err(_) => tracing::error!("flush on shutdown timed out after 10 s"),
    }
}

/// Logs a startup failure straight to stderr (not through tracing, so RUST_LOG cannot hide it)
/// and exits. The runtime image is distroless with no shell, so this line is the only clue.
fn fatal(code: i32, msg: &str) -> ! {
    eprintln!("core-service: fatal: {msg}");
    std::process::exit(code);
}

/// Drives time-based rules (expiry, decay) and, in mock mode, the generator.
fn spawn_ticker(
    engine: std::sync::Arc<engine::Engine>,
    every: std::time::Duration,
) -> tokio::task::JoinHandle<()> {
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
    })
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

/// `core-service backup [dest] [--overwrite]`: consistent online copy with `VACUUM INTO` (safe in WAL mode while the
/// service runs). Works in the distroless image: `docker compose exec core-service /app backup`.
/// Default destination `/data/backup.db` (replaced each time); prints the path and the result
/// of `PRAGMA integrity_check` on the copy.
async fn backup() {
    let src = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "/data/core.db".into());
    let args: Vec<String> = std::env::args().skip(2).collect();
    let overwrite = args.iter().any(|a| a == "--overwrite");
    let dest = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "/data/backup.db".into());
    match run_backup(&src, &dest, overwrite).await {
        Ok(line) => println!("{line}"),
        Err(e) => fatal_backup(&e),
    }
}

/// Absolute, symlink-free form of `p`, also when the file does not exist yet.
fn resolve(p: &std::path::Path) -> std::path::PathBuf {
    if let Ok(c) = p.canonicalize() {
        return c;
    }
    let parent = p
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let parent = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    parent.join(p.file_name().unwrap_or_default())
}

/// Refuses a destination that is the live database (or one of its WAL/SHM/journal files), and an
/// existing destination unless `overwrite` is set. The source is never deleted or written.
pub fn check_backup_target(src: &str, dest: &str, overwrite: bool) -> Result<(), String> {
    let (s, d) = (
        resolve(std::path::Path::new(src)),
        resolve(std::path::Path::new(dest)),
    );
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut live = s.clone().into_os_string();
        live.push(suffix);
        if d.as_os_str() == live {
            return Err(format!(
                "destination {dest} is the live database ({src}{suffix}); choose another path"
            ));
        }
    }
    if d.exists() && !overwrite {
        return Err(format!(
            "destination {dest} already exists; remove it or pass --overwrite"
        ));
    }
    Ok(())
}

/// `VACUUM INTO` copy of `src` at `dest` plus `PRAGMA integrity_check` on the copy.
pub async fn run_backup(src: &str, dest: &str, overwrite: bool) -> Result<String, String> {
    check_backup_target(src, dest, overwrite)?;
    if overwrite {
        // Safe: check_backup_target proved `dest` is not the live database or its side files.
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{dest}{suffix}"));
        }
    }
    let db = db::open(src)
        .await
        .map_err(|e| format!("cannot open {src}: {e}"))?;
    let res = sqlx::query("VACUUM INTO ?").bind(dest).execute(&db).await;
    db.close().await;
    res.map_err(|e| format!("VACUUM INTO failed: {e}"))?;
    let copy = db::open(dest)
        .await
        .map_err(|e| format!("cannot open the copy: {e}"))?;
    let check: (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(&copy)
        .await
        .map_err(|e| format!("integrity check failed: {e}"))?;
    copy.close().await;
    if check.0 != "ok" {
        return Err(format!("integrity check of {dest}: {}", check.0));
    }
    Ok(format!("backup {dest} integrity=ok"))
}

fn fatal_backup(msg: &str) -> ! {
    eprintln!("core-service: backup failed: {msg}");
    std::process::exit(1);
}

/// glibc keeps freed large buffers (Argon2 uses 8 MiB per hash) in per-thread arenas, which can
/// add up past the container's memory limit. A fixed mmap threshold returns them to the OS on
/// free, and two arenas cap the per-thread growth. No-op outside glibc (e.g. macOS dev builds).
fn tune_malloc() {
    #[cfg(target_env = "gnu")]
    // SAFETY: mallopt only changes allocator tunables; called once before other threads start.
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, 1 << 20);
        libc::mallopt(libc::M_ARENA_MAX, 2);
    }
}

#[cfg(test)]
mod contract_copy {
    /// The gateway must not trust CF-Connecting-IP from whole private ranges (review finding 5).
    #[test]
    fn gateway_trusts_only_generated_proxy_list() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        let conf = std::fs::read_to_string(format!("{root}/gateway/nginx.conf")).unwrap();
        let directives: Vec<&str> = conf
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("set_real_ip_from"))
            .collect();
        assert!(directives.is_empty(), "hard-coded trust: {directives:?}");
        assert!(conf.contains("include /etc/nginx/trusted_proxies.conf;"));
        let script =
            std::fs::read_to_string(format!("{root}/gateway/entrypoint/40-trusted-proxies.sh"))
                .unwrap();
        for broad in ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"] {
            assert!(!script.contains(broad), "{broad} in the generator");
        }
    }

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

#[cfg(test)]
mod backup_tests {
    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ks-backup-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[tokio::test]
    async fn never_touches_the_source() {
        let dir = tmp();
        let src = dir.join("live.db");
        let src_s = src.to_str().unwrap().to_string();
        let db = crate::db::open(&src_s).await.unwrap();
        db.close().await;
        let before = std::fs::read(&src).unwrap();
        for dest in [
            src_s.clone(),
            format!("{src_s}-wal"),
            dir.join("./live.db").to_str().unwrap().to_string(),
        ] {
            for overwrite in [false, true] {
                let err = super::run_backup(&src_s, &dest, overwrite)
                    .await
                    .unwrap_err();
                assert!(err.contains("live database"), "{dest}: {err}");
            }
        }
        assert!(
            src.exists() && std::fs::read(&src).unwrap() == before,
            "source untouched"
        );

        let dest = dir.join("copy.db");
        let dest_s = dest.to_str().unwrap();
        assert!(
            super::run_backup(&src_s, dest_s, false)
                .await
                .unwrap()
                .ends_with("integrity=ok")
        );
        let err = super::run_backup(&src_s, dest_s, false).await.unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        assert!(
            super::run_backup(&src_s, dest_s, true).await.is_ok(),
            "--overwrite replaces a copy"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
