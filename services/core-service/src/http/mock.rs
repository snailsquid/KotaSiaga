//! Demo controls, mounted only when MOCK_MODE=true (otherwise these paths are 404).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;

use super::envelope::{Envelope, Meta};
use super::error::{ApiError, ErrorCode};
use crate::AppState;
use crate::mock::{MockState, SCENARIOS};
use crate::risk::grid::CellId;

fn not_found() -> ApiError {
    ApiError::new(ErrorCode::NotFound, "Endpoint tidak ditemukan")
}

fn envelope(st: &AppState, data: MockState) -> Json<Envelope<MockState>> {
    let s = st.engine.snapshot();
    Json(Envelope {
        data,
        meta: Meta {
            seq: s.seq,
            updated_at: s.updated_at,
            source: s.source,
            stale: s.stale,
            reset: None,
        },
    })
}

pub async fn state(State(st): State<AppState>) -> Result<Json<Envelope<MockState>>, ApiError> {
    let g = st.engine.mock.as_ref().ok_or_else(not_found)?;
    let now = chrono::Utc::now();
    let data = st.engine.update(now, |w| {
        g.lock().unwrap_or_else(|e| e.into_inner()).state(w, now)
    });
    Ok(envelope(&st, data))
}

#[derive(Deserialize)]
pub struct ScenarioRequest {
    name: String,
    lat: Option<f64>,
    lon: Option<f64>,
}

pub async fn scenario(
    State(st): State<AppState>,
    body: Result<Json<ScenarioRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<(StatusCode, Json<Envelope<MockState>>), ApiError> {
    let g = st.engine.mock.as_ref().ok_or_else(not_found)?;
    let Json(req) = body
        .map_err(|e| ApiError::new(ErrorCode::ValidationError, format!("Body tidak valid: {e}")))?;
    if !SCENARIOS.contains(&req.name.as_str()) {
        return Err(ApiError::new(
            ErrorCode::ValidationError,
            format!("name harus salah satu dari: {}", SCENARIOS.join(", ")),
        ));
    }
    let at = match (req.lat, req.lon) {
        (Some(lat), Some(lon)) => {
            CellId::of(lat, lon).ok_or_else(|| {
                ApiError::new(ErrorCode::ValidationError, "Lokasi di luar wilayah layanan")
            })?;
            Some((lat, lon))
        }
        (None, None) => None,
        _ => {
            return Err(ApiError::new(
                ErrorCode::ValidationError,
                "lat dan lon harus diisi bersama",
            ));
        }
    };
    let now = chrono::Utc::now();
    let data = st.engine.update(now, |w| {
        let mut g = g.lock().unwrap_or_else(|e| e.into_inner());
        g.scenario(w, &req.name, at, now).map(|_| g.state(w, now))
    });
    let data = data.map_err(|m| ApiError::new(ErrorCode::ValidationError, m))?;
    tracing::info!(scenario = req.name, "mock scenario triggered");
    Ok((StatusCode::ACCEPTED, envelope(&st, data)))
}
