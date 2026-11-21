//! HTTP layer: routing and envelopes only. No domain rules here.

// Used by the bounded-context handlers as they land.
#[allow(dead_code)]
pub mod envelope;
#[allow(dead_code)]
pub mod error;

use axum::{Json, Router, routing::get};
use serde_json::{Value, json};

use crate::AppState;
use error::{ApiError, ErrorCode};

/// All routes are served under `/api` so local paths match the public base URL.
pub fn router(state: AppState) -> Router {
    let api = Router::new().route("/health", get(health));
    Router::new()
        .nest("/api", api)
        .fallback(|| async { ApiError::new(ErrorCode::NotFound, "Endpoint tidak ditemukan") })
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn state() -> AppState {
        let config =
            crate::config::Config::from_lookup(|k| (k == "MOCK_SEED").then(|| "1".into())).unwrap();
        AppState {
            config: std::sync::Arc::new(config),
            db: crate::db::open(":memory:").await.unwrap(),
        }
    }

    async fn call(uri: &str) -> (StatusCode, Value) {
        let res = router(state().await)
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn health_is_ok() {
        assert_eq!(
            call("/api/health").await,
            (StatusCode::OK, json!({"status":"ok"}))
        );
    }

    #[tokio::test]
    async fn unknown_path_uses_error_envelope() {
        let (status, body) = call("/api/nope").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "not_found");
        assert!(body["error"]["request_id"].is_string());
    }
}
