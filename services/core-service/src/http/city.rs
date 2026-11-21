//! City data for other groups and the app: /status, /risk, /weather, /disasters, /reports, /changes.
//! Handlers only read the latest snapshot; no domain rules here.

use axum::Json;
use axum::extract::{Query as AxQuery, State};
use serde::Serialize;

use super::envelope::{Envelope, Meta};
use super::error::{ApiError, ErrorCode};
use super::params::{self, Query};
use crate::AppState;
use crate::engine::Snapshot;
use crate::feed;
use crate::reports::{Category, ReportStatus};
use crate::risk::grid::distance_m;
use crate::risk::inputs::{DisasterStatus, WeatherView};
use crate::risk::{CellRiskView, RiskLevel, report_weight};
use crate::rules::MAX_BBOX_CELLS;

type ApiResult<T> = Result<Json<Envelope<T>>, ApiError>;

fn meta(s: &Snapshot) -> Meta {
    Meta {
        seq: s.seq,
        updated_at: s.updated_at,
        source: s.source,
        stale: s.stale,
        reset: None,
    }
}

fn ok<T: Serialize>(s: &Snapshot, data: T) -> ApiResult<T> {
    Ok(Json(Envelope {
        data,
        meta: meta(s),
    }))
}

fn weather_view(s: &Snapshot, lat: f64, lon: f64) -> Result<WeatherView, ApiError> {
    if !s.weather.available {
        return Err(ApiError::new(
            ErrorCode::UpstreamUnavailable,
            "Data cuaca belum tersedia",
        ));
    }
    Ok(s.weather.view_at(lat, lon))
}

pub async fn risk(
    State(st): State<AppState>,
    AxQuery(q): AxQuery<Query>,
) -> Result<Json<Envelope<serde_json::Value>>, ApiError> {
    let s = st.engine.snapshot();
    let point = params::point_opt(&q)?;
    let bbox = params::bbox(&q)?;
    let data = match (point, bbox) {
        (Some((_, _, cell)), None) => {
            serde_json::to_value(CellRiskView::new(cell, s.risk.get(cell)))
        }
        (None, Some(b)) => {
            let cells = b.cells(MAX_BBOX_CELLS).ok_or_else(|| {
                ApiError::new(
                    ErrorCode::ValidationError,
                    format!("bbox terlalu besar (maksimal {MAX_BBOX_CELLS} sel)"),
                )
            })?;
            serde_json::to_value(
                cells
                    .into_iter()
                    .map(|c| CellRiskView::new(c, s.risk.get(c)))
                    .collect::<Vec<_>>(),
            )
        }
        _ => {
            return Err(ApiError::new(
                ErrorCode::ValidationError,
                "Isi lat dan lon, atau bbox (salah satu)",
            ));
        }
    }
    .map_err(|_| ApiError::new(ErrorCode::UpstreamUnavailable, "Gagal menyusun data risiko"))?;
    Ok(Json(Envelope {
        data,
        meta: meta(&s),
    }))
}

pub async fn weather(
    State(st): State<AppState>,
    AxQuery(q): AxQuery<Query>,
) -> ApiResult<WeatherView> {
    let s = st.engine.snapshot();
    let (lat, lon, _) = params::point(&q)?;
    let w = weather_view(&s, lat, lon)?;
    ok(&s, w)
}

#[derive(Serialize)]
pub struct StatusView {
    cell_id: String,
    risk_level: RiskLevel,
    weather: WeatherView,
    active_reports: usize,
    disaster_signals: usize,
}

pub async fn status(
    State(st): State<AppState>,
    AxQuery(q): AxQuery<Query>,
) -> ApiResult<StatusView> {
    let s = st.engine.snapshot();
    let (lat, lon, cell) = params::point(&q)?;
    let radius = params::int_opt(&q, "radius_m", 100, 10_000)?.unwrap_or(1000) as f64;
    let near = |la: f64, lo: f64| distance_m((lat, lon), (la, lo)) <= radius;
    let now = chrono::Utc::now();
    ok(
        &s,
        StatusView {
            cell_id: cell.to_string(),
            risk_level: s.risk.get(cell).level,
            weather: weather_view(&s, lat, lon)?,
            active_reports: s
                .reports
                .iter()
                .filter(|r| report_weight(r, now) > 0.0 && near(r.lat, r.lon))
                .count(),
            disaster_signals: s
                .disasters
                .iter()
                .filter(|d| d.status == DisasterStatus::Active && near(d.lat, d.lon))
                .count(),
        },
    )
}

pub async fn disasters(
    State(st): State<AppState>,
    AxQuery(q): AxQuery<Query>,
) -> ApiResult<Vec<crate::risk::inputs::DisasterSignal>> {
    let s = st.engine.snapshot();
    let bbox = params::bbox(&q)?;
    let (limit, offset) = params::limit_offset(&q)?;
    let data = s
        .disasters
        .iter()
        .filter(|d| bbox.is_none_or(|b| b.contains(d.lat, d.lon)))
        .skip(offset)
        .take(limit)
        .cloned()
        .collect();
    ok(&s, data)
}

pub async fn reports(
    State(st): State<AppState>,
    AxQuery(q): AxQuery<Query>,
) -> ApiResult<Vec<crate::reports::Report>> {
    let s = st.engine.snapshot();
    let category = match q.get("category") {
        None => None,
        Some(c) => Some(Category::parse(c).ok_or_else(|| {
            ApiError::new(
                ErrorCode::ValidationError,
                "category harus flood, accident, road_damage, road_closure, atau event_crowd",
            )
        })?),
    };
    let status = match q.get("status") {
        None => None,
        Some(v) => Some(ReportStatus::parse(v).ok_or_else(|| {
            ApiError::new(
                ErrorCode::ValidationError,
                "status harus waiting, verified, atau expired",
            )
        })?),
    };
    let bbox = params::bbox(&q)?;
    let since = params::time_opt(&q, "since")?;
    let (limit, offset) = params::limit_offset(&q)?;
    let data = s
        .reports
        .iter()
        .filter(|r| category.is_none_or(|c| r.category == c))
        .filter(|r| status.is_none_or(|v| r.status == v))
        .filter(|r| bbox.is_none_or(|b| b.contains(r.lat, r.lon)))
        .filter(|r| since.is_none_or(|t| r.created_at >= t))
        .skip(offset)
        .take(limit)
        .cloned()
        .collect();
    ok(&s, data)
}

pub async fn changes(
    State(st): State<AppState>,
    AxQuery(q): AxQuery<Query>,
) -> ApiResult<Vec<feed::Change>> {
    let s = st.engine.snapshot();
    let since = params::int_opt(&q, "since", 0, i64::MAX)?
        .ok_or_else(|| ApiError::new(ErrorCode::ValidationError, "since wajib diisi"))?
        as u64;
    let (limit, _) = params::limit_offset(&q)?;
    let (data, reset) = feed::read(&s.changes, s.seq, since, limit);
    let mut m = meta(&s);
    m.reset = reset.then_some(true);
    Ok(Json(Envelope { data, meta: m }))
}
