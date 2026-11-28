//! KotaSiaga Layanan Integrasi (Tugas 3). Runs off the STB (Coolify on volta,
//! https://kotasiaga.arkk.dev) and combines three services for the mobile app:
//! - KotaSiaga, our own microservice on the STB (https://2bintang1hati.stei.site/api):
//!   risk per point, safe routes, city data, user accounts and reports.
//! - JelajahLokal (Tourism) `GET /feed/destinations` -> monitor points.
//! - DonorKita (Health) `GET /donation-events` -> destination points for safe routes.
//!
//! Every outbound call goes through `upstream::Client` (timeout, retry, call log); partner
//! payloads are parsed by `partner-contracts`; requests read memory only.

mod api;
mod config;
mod state;
#[cfg(test)]
mod tests;
mod upstream;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tracing_subscriber::EnvFilter;

fn every(secs: u64) -> tokio::time::Interval {
    let mut i = tokio::time::interval(Duration::from_secs(secs));
    i.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    i
}

/// Partner feeds every `PARTNER_REFRESH_SECONDS` (default 300 s, the agreed 5 minute TTL), or
/// every 30 s while mocked so changes show up; risk for every partner point every
/// `RISK_REFRESH_SECONDS` (default 30 s; KotaSiaga's risk TTL is 60 s).
fn spawn_loops(app: Arc<state::App>) {
    for which in ["tourism", "health"] {
        let a = app.clone();
        let mocked = if which == "tourism" {
            a.cfg.tourism.mock
        } else {
            a.cfg.health.mock
        };
        let secs = if mocked {
            30.min(a.cfg.partner_refresh_s)
        } else {
            a.cfg.partner_refresh_s
        };
        tokio::spawn(async move {
            let mut t = every(secs);
            loop {
                t.tick().await;
                a.maybe_fault();
                if which == "tourism" {
                    a.refresh_tourism(Utc::now()).await;
                } else {
                    a.refresh_health(Utc::now()).await;
                }
            }
        });
    }
    let a = app;
    tokio::spawn(async move {
        // Let the first partner fetch land before the first risk round.
        tokio::time::sleep(Duration::from_secs(2)).await;
        let mut t = every(a.cfg.risk_refresh_s);
        loop {
            t.tick().await;
            a.refresh_risk().await;
        }
    });
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .init();
    let cfg = config::Config::from_env().unwrap_or_else(|e| {
        eprintln!("integration-service: fatal: invalid configuration: {e}");
        std::process::exit(2);
    });
    if cfg.kotasiaga_api_key.is_none() {
        tracing::warn!(
            "KOTASIAGA_API_KEY not set: risk enrichment and keyed reads for API-key callers will fail"
        );
    }
    tracing::info!(
        kotasiaga = %cfg.kotasiaga_base_url,
        tourism = if cfg.tourism.mock { "mock" } else { "live" },
        health = if cfg.health.mock { "mock" } else { "live" },
        mock_mode = cfg.mock_mode,
        seed = cfg.mock_seed,
        "integration-service upstreams"
    );
    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    let app = Arc::new(state::App::new(cfg));
    spawn_loops(app.clone());
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("integration-service: fatal: cannot bind {addr}: {e}");
            std::process::exit(1);
        });
    tracing::info!(%addr, "integration-service listening");
    let svc = api::router(app).into_make_service_with_connect_info::<SocketAddr>();
    if let Err(e) = axum::serve(listener, svc)
        .with_graceful_shutdown(shutdown())
        .await
    {
        eprintln!("integration-service: fatal: server error: {e}");
        std::process::exit(1);
    }
}

async fn shutdown() {
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
