//! Who is calling: a group (`X-API-Key`), the admin key, or an app user (`Authorization: Bearer`).
//! Also writes one access-log line per request (who, what, status, ms).

use std::time::Instant;

use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::error::{ApiError, ErrorCode};
use crate::AppState;

/// Name for the access log: the group name, `admin`, or `user:<id>`.
#[derive(Clone)]
pub struct Caller(pub String);

/// An app user authenticated by a JWT from identity-service.
#[derive(Clone, Debug)]
pub struct User {
    pub id: String,
    #[allow(dead_code)] // kept for logs and future use; never used for authorization
    pub name: String,
}

fn bearer(req: &Request) -> Option<&str> {
    let v = req.headers().get("authorization")?.to_str().ok()?;
    v.strip_prefix("Bearer ")
        .or_else(|| v.strip_prefix("bearer "))
        .map(str::trim)
}

fn verify_user(state: &AppState, token: &str) -> Result<User, ApiError> {
    let secret = state.config.jwt_secret.as_deref().ok_or_else(|| {
        ApiError::new(
            ErrorCode::InvalidToken,
            "Login belum dikonfigurasi di server",
        )
    })?;
    auth_token::verify(token, secret, chrono::Utc::now().timestamp())
        .map(|c| User {
            id: c.sub,
            name: c.name,
        })
        .map_err(|e| ApiError::new(ErrorCode::InvalidToken, format!("Token tidak valid: {e}")))
}

async fn run_as(caller: Caller, mut req: Request, next: Next) -> Response {
    req.extensions_mut().insert(caller.clone());
    let mut res = next.run(req).await;
    res.extensions_mut().insert(caller); // for the access log
    res
}

/// Read endpoints and `/routes/safe`: a group key, the admin key, or a user token.
pub async fn require_key_or_user(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    if let Some(key) = req.headers().get("x-api-key").and_then(|v| v.to_str().ok()) {
        return match state.config.caller(key) {
            Some(name) => run_as(Caller(name.to_string()), req, next).await,
            None => ApiError::new(ErrorCode::InvalidApiKey, "API key tidak valid").into_response(),
        };
    }
    if let Some(token) = bearer(&req) {
        return match verify_user(&state, token) {
            Ok(user) => {
                let caller = Caller(format!("user:{}", user.id));
                req.extensions_mut().insert(user);
                run_as(caller, req, next).await
            }
            Err(e) => e.into_response(),
        };
    }
    ApiError::new(
        ErrorCode::InvalidApiKey,
        "API key tidak valid (header X-API-Key atau Authorization: Bearer wajib)",
    )
    .into_response()
}

/// User-only endpoints (create report, vote, subscriptions, alerts): a valid user JWT.
pub async fn require_user(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let Some(token) = bearer(&req) else {
        return ApiError::new(
            ErrorCode::InvalidToken,
            "Login dulu: header Authorization: Bearer <token> wajib",
        )
        .into_response();
    };
    match verify_user(&state, token) {
        Ok(user) => {
            let caller = Caller(format!("user:{}", user.id));
            req.extensions_mut().insert(user);
            run_as(caller, req, next).await
        }
        Err(e) => e.into_response(),
    }
}

pub async fn require_admin(req: Request, next: Next) -> Response {
    match req.extensions().get::<Caller>() {
        Some(Caller(name)) if name == "admin" => next.run(req).await,
        _ => ApiError::new(ErrorCode::Forbidden, "Hanya untuk admin").into_response(),
    }
}

impl<S: Send + Sync> FromRequestParts<S> for User {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<User>()
            .cloned()
            .ok_or_else(|| ApiError::new(ErrorCode::InvalidToken, "Login dulu"))
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
