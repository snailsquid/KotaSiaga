//! KotaSiaga core-service: the five business bounded contexts as modules in one process.
//!
//! Layout rules (see AGENTS.md):
//! - Each context owns its own tables and talks to other contexts only through domain events.
//! - External APIs (Open-Meteo, PetaBencana, OSRM, Tourism, Health) are called only from `adapters`.
//! - Hot read data lives in memory and is recomputed on domain events, never per request.

mod adapters;
mod config;
mod db;
mod feed;
mod http;
mod mock;
mod reports;
mod risk;
mod routing;
mod verification;

use std::net::SocketAddr;

use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub config: std::sync::Arc<config::Config>,
    pub db: sqlx::SqlitePool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = match config::Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("invalid configuration: {e}");
            std::process::exit(2);
        }
    };
    if config.mock.enabled {
        tracing::info!(
            seed = config.mock.seed,
            tick_seconds = config.mock.tick_seconds,
            time_scale = config.mock.time_scale,
            "MOCK_MODE enabled"
        );
    }

    let db = db::open(&config.database_path).await.unwrap_or_else(|e| {
        tracing::error!("cannot open database {}: {e}", config.database_path);
        std::process::exit(1);
    });

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let state = AppState {
        config: std::sync::Arc::new(config),
        db,
    };
    let app = http::router(state);

    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    tracing::info!(%addr, "core-service listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server");
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
