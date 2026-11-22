//! KotaSiaga identity-service (Identitas & Akses): app user accounts and JWT issuance.
//! Runs in its own container with its own SQLite file. core-service verifies the tokens
//! it issues with the same `JWT_SECRET`; group API keys (`X-API-Key`) live in core's config.

mod config;
mod http;
mod password;
mod store;

use std::net::SocketAddr;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .init();

    let config = config::Config::from_env()
        .unwrap_or_else(|e| fatal(&format!("invalid configuration: {e}")));
    if config.jwt_secret.is_none() {
        tracing::error!(
            "JWT_SECRET is not set (or shorter than {} bytes): register/login return 503 until it is",
            auth_token::MIN_SECRET_LEN
        );
    }
    let db = store::open(&config.database_path).await.unwrap_or_else(|e| {
        fatal(&format!(
            "cannot open database {}: {e} (does its directory exist and is it writable by this user?)",
            config.database_path
        ))
    });
    if let Some(acc) = &config.test_account {
        match store::upsert_test_account(&db, acc).await {
            Ok(id) => tracing::info!(email = acc.email, user_id = id, "test account ready"),
            Err(e) => tracing::error!(error = %e, "could not seed the test account"),
        }
    } else {
        tracing::warn!("TEST_ACCOUNT_PASSWORD not set: no test account seeded");
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let app = http::router(http::AppState::new(config, db));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| fatal(&format!("cannot bind {addr}: {e}")));
    tracing::info!(%addr, "identity-service listening");
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        fatal(&format!("server error: {e}"));
    }
}

/// Logs a startup failure straight to stderr (not through tracing, so RUST_LOG cannot hide it)
/// and exits. The runtime image is distroless with no shell, so this line is the only clue.
fn fatal(msg: &str) -> ! {
    eprintln!("identity-service: fatal: {msg}");
    std::process::exit(1);
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
