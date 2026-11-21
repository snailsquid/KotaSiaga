//! `X-API-Key` check and one log line per request (who, what, status, ms).
//! User JWTs are accepted here once Identitas & Akses issues them.

use std::time::Instant;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::error::{ApiError, ErrorCode};
use crate::AppState;

#[derive(Clone)]
pub struct Caller(pub String);

pub async fn require_api_key(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let key = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    match state.config.caller(key) {
        Some(name) => {
            let caller = Caller(name.to_string());
            req.extensions_mut().insert(caller.clone());
            let mut res = next.run(req).await;
            res.extensions_mut().insert(caller); // for the access log
            res
        }
        None => ApiError::new(ErrorCode::InvalidApiKey, "API key tidak valid").into_response(),
    }
}

pub async fn require_admin(req: Request, next: Next) -> Response {
    match req.extensions().get::<Caller>() {
        Some(Caller(name)) if name == "admin" => next.run(req).await,
        _ => ApiError::new(ErrorCode::Forbidden, "Hanya untuk admin").into_response(),
    }
}

pub async fn log_request(req: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let res = next.run(req).await;
    let caller = res
        .extensions()
        .get::<Caller>()
        .map(|c| c.0.clone())
        .unwrap_or_else(|| "-".into());
    tracing::info!(
        target: "access",
        %method,
        path,
        status = res.status().as_u16(),
        ms = started.elapsed().as_millis() as u64,
        caller,
    );
    res
}
