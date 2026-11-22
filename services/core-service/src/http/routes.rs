//! POST /routes/safe (Rute Aman), GET /monitor-points (TitikPantau, from Tourism) and
//! GET /destination-points (TitikTujuan, from Health).

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Query as AxQuery, State};
use serde::{Deserialize, Serialize};

use super::envelope::{Envelope, Meta, Source};
use super::error::{ApiError, ErrorCode};
use super::params::{self, Query};
use crate::AppState;
use crate::adapters::partners::{DestinationPoint, MonitorPoint};
use crate::risk::RiskLevel;
use crate::risk::grid::CellId;
use crate::routing::{self, SafeRouteResult};

fn invalid(msg: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::ValidationError, msg)
}

#[derive(Deserialize)]
pub struct PointIn {
    lat: f64,
    lon: f64,
}

#[derive(Deserialize)]
pub struct SafeRouteRequest {
    origin: PointIn,
    destination: Option<PointIn>,
    /// A destination point id from `/destination-points` (e.g. a donation event), instead of
    /// `destination` coordinates.
    destination_id: Option<String>,
    alternatives: Option<i64>,
}

fn point(p: &PointIn, name: &str) -> Result<(f64, f64), ApiError> {
    if !(p.lat.is_finite() && p.lon.is_finite()) || CellId::of(p.lat, p.lon).is_none() {
        return Err(invalid(format!(
            "{name} di luar wilayah layanan (Bandung Raya)"
        )));
    }
    Ok((p.lat, p.lon))
}

pub async fn safe_route(
    State(st): State<AppState>,
    b: Result<Json<SafeRouteRequest>, JsonRejection>,
) -> Result<Json<Envelope<SafeRouteResult>>, ApiError> {
    let Json(req) = b.map_err(|e| invalid(format!("Body tidak valid: {}", e.body_text())))?;
    let origin = point(&req.origin, "origin")?;
    let destination = match (&req.destination, &req.destination_id) {
        (Some(d), None) => point(d, "destination")?,
        (None, Some(id)) => {
            let d = st.partners.destination(id).ok_or_else(|| {
                ApiError::new(
                    ErrorCode::NotFound,
                    "destination_id tidak ditemukan di /destination-points",
                )
            })?;
            (d.lat, d.lon)
        }
        _ => return Err(invalid("Isi destination atau destination_id (salah satu)")),
    };
    let n = req.alternatives.unwrap_or(3);
    if !(1..=3).contains(&n) {
        return Err(invalid("alternatives harus 1 sampai 3"));
    }
    let cands = st
        .routes
        .candidates(origin, destination, n as u8)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "no route candidates");
            ApiError::new(
                ErrorCode::UpstreamUnavailable,
                "Layanan rute (OSRM) sedang tidak tersedia, coba lagi nanti",
            )
            .with_retry_after(30)
        })?;
    let s = st.engine.snapshot();
    let now = chrono::Utc::now();
    let result = routing::score(cands.routes, &s.risk, &s.reports, now).ok_or_else(|| {
        ApiError::new(
            ErrorCode::NotFound,
            "Tidak ada rute jalan di antara kedua titik",
        )
    })?;
    Ok(Json(Envelope {
        data: result,
        meta: Meta {
            seq: s.seq,
            updated_at: s.updated_at,
            source: s.source,
            stale: s.stale || cands.stale,
            reset: None,
        },
    }))
}

#[derive(Serialize)]
pub struct WithRisk<T: Serialize> {
    #[serde(flatten)]
    point: T,
    cell_id: String,
    risk_level: RiskLevel,
    rain_mm_h: f64,
}

fn enrich<T: Serialize>(
    s: &crate::engine::Snapshot,
    point: T,
    lat: f64,
    lon: f64,
) -> Option<WithRisk<T>> {
    let cell = CellId::of(lat, lon)?;
    let c = s.risk.get(cell);
    Some(WithRisk {
        point,
        cell_id: cell.to_string(),
        risk_level: c.level,
        rain_mm_h: (c.rain_mm_h * 10.0).round() / 10.0,
    })
}

fn partner_meta(
    s: &crate::engine::Snapshot,
    mock: bool,
    stale: bool,
    fetched: bool,
) -> Result<Meta, ApiError> {
    if !fetched {
        return Err(ApiError::new(
            ErrorCode::UpstreamUnavailable,
            "Data mitra belum tersedia, coba lagi sebentar",
        )
        .with_retry_after(10));
    }
    Ok(Meta {
        seq: s.seq,
        updated_at: s.updated_at,
        source: if mock { Source::Mock } else { s.source },
        stale: stale || s.stale,
        reset: None,
    })
}

fn risk_filter(q: &Query) -> Result<Option<RiskLevel>, ApiError> {
    match q.get("risk_level").map(String::as_str) {
        None => Ok(None),
        Some("safe") => Ok(Some(RiskLevel::Safe)),
        Some("caution") => Ok(Some(RiskLevel::Caution)),
        Some("danger") => Ok(Some(RiskLevel::Danger)),
        Some(_) => Err(invalid("risk_level harus safe, caution, atau danger")),
    }
}

pub async fn monitor_points(
    State(st): State<AppState>,
    AxQuery(q): AxQuery<Query>,
) -> Result<Json<Envelope<Vec<WithRisk<MonitorPoint>>>>, ApiError> {
    let bbox = params::bbox(&q)?;
    let level = risk_filter(&q)?;
    let (limit, offset) = params::limit_offset(&q)?;
    let feed = st.partners.monitor_points();
    let s = st.engine.snapshot();
    let meta = partner_meta(&s, feed.mock, feed.stale, feed.fetched_at.is_some())?;
    let data = feed
        .items
        .into_iter()
        .filter(|p| bbox.is_none_or(|b| b.contains(p.lat, p.lon)))
        .filter_map(|p| {
            let (lat, lon) = (p.lat, p.lon);
            enrich(&s, p, lat, lon)
        })
        .filter(|p| level.is_none_or(|l| p.risk_level == l))
        .skip(offset)
        .take(limit)
        .collect();
    Ok(Json(Envelope { data, meta }))
}

pub async fn destination_points(
    State(st): State<AppState>,
    AxQuery(q): AxQuery<Query>,
) -> Result<Json<Envelope<Vec<WithRisk<DestinationPoint>>>>, ApiError> {
    let bbox = params::bbox(&q)?;
    let level = risk_filter(&q)?;
    let (limit, offset) = params::limit_offset(&q)?;
    let feed = st.partners.destination_points();
    let s = st.engine.snapshot();
    let meta = partner_meta(&s, feed.mock, feed.stale, feed.fetched_at.is_some())?;
    let data = feed
        .items
        .into_iter()
        .filter(|p| bbox.is_none_or(|b| b.contains(p.lat, p.lon)))
        .filter_map(|p| {
            let (lat, lon) = (p.lat, p.lon);
            enrich(&s, p, lat, lon)
        })
        .filter(|p| level.is_none_or(|l| p.risk_level == l))
        .skip(offset)
        .take(limit)
        .collect();
    Ok(Json(Envelope { data, meta }))
}
