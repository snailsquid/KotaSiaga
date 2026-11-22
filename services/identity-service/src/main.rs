//! KotaSiaga identity-service (Identitas & Akses): user JWT and per-group API keys.
//! Runs in its own container with its own SQLite file.

use std::net::SocketAddr;

use axum::{Json, Router, routing::get};
use tracing_subscriber::EnvFilter;

fn router() -> Router {
    Router::new().nest(
        "/api",
        Router::new().route("/health", get(|| async { Json(serde_json_ok()) })),
    )
}

fn serde_json_ok() -> std::collections::BTreeMap<&'static str, &'static str> {
    [("status", "ok")].into_iter().collect()
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .init();
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8081);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| fatal(&format!("cannot bind {addr}: {e}")));
    tracing::info!(%addr, "identity-service listening");
    if let Err(e) = axum::serve(listener, router())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_is_ok() {
        let res = router()
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..], br#"{"status":"ok"}"#);
    }
}
