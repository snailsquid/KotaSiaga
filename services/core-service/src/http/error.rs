//! Error envelope and codes from AGENTS.md section 3.

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidApiKey,
    InvalidToken,
    Forbidden,
    NotFound,
    ValidationError,
    Conflict,
    RateLimited,
    UpstreamUnavailable,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidApiKey => "invalid_api_key",
            Self::InvalidToken => "invalid_token",
            Self::Conflict => "conflict",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::ValidationError => "validation_error",
            Self::RateLimited => "rate_limited",
            Self::UpstreamUnavailable => "upstream_unavailable",
        }
    }

    pub fn status(self) -> StatusCode {
        match self {
            Self::InvalidApiKey | Self::InvalidToken => StatusCode::UNAUTHORIZED,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::ValidationError => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::UpstreamUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

#[derive(Debug)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    pub retry_after_s: Option<u32>,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retry_after_s: None,
        }
    }
}

impl ApiError {
    pub fn with_retry_after(mut self, s: u32) -> Self {
        self.retry_after_s = Some(s);
        self
    }

    /// Database trouble: logged here, shown to the client as a plain 503.
    pub fn db(e: impl std::fmt::Display) -> Self {
        tracing::error!(error = %e, "database error");
        Self::new(
            ErrorCode::UpstreamUnavailable,
            "Penyimpanan data sedang bermasalah",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": {
                "code": self.code.as_str(),
                "message": self.message,
                "request_id": uuid::Uuid::new_v4().to_string(),
            }
        });
        let mut res = (self.code.status(), axum::Json(body)).into_response();
        if let Some(s) = self.retry_after_s
            && let Ok(v) = HeaderValue::from_str(&s.to_string())
        {
            res.headers_mut().insert(header::RETRY_AFTER, v);
        }
        res
    }
}
