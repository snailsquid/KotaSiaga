//! Query parsing with contract-shaped `validation_error`s (axum's default rejections are plain text).

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use super::error::{ApiError, ErrorCode};
use crate::risk::grid::{Bbox, CellId};

pub type Query = HashMap<String, String>;

fn invalid(msg: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::ValidationError, msg)
}

pub fn f64_opt(q: &Query, name: &str, min: f64, max: f64) -> Result<Option<f64>, ApiError> {
    match q.get(name) {
        None => Ok(None),
        Some(v) => match v.parse::<f64>() {
            Ok(x) if x.is_finite() && (min..=max).contains(&x) => Ok(Some(x)),
            _ => Err(invalid(format!(
                "{name} harus angka antara {min} dan {max}"
            ))),
        },
    }
}

pub fn int_opt(q: &Query, name: &str, min: i64, max: i64) -> Result<Option<i64>, ApiError> {
    match q.get(name) {
        None => Ok(None),
        Some(v) => match v.parse::<i64>() {
            Ok(x) if (min..=max).contains(&x) => Ok(Some(x)),
            _ => Err(invalid(format!(
                "{name} harus bilangan bulat antara {min} dan {max}"
            ))),
        },
    }
}

/// `lat` and `lon`, both present, inside the service area.
pub fn point_opt(q: &Query) -> Result<Option<(f64, f64, CellId)>, ApiError> {
    let lat = f64_opt(q, "lat", -90.0, 90.0)?;
    let lon = f64_opt(q, "lon", -180.0, 180.0)?;
    match (lat, lon) {
        (None, None) => Ok(None),
        (Some(lat), Some(lon)) => {
            let cell = CellId::of(lat, lon)
                .ok_or_else(|| invalid("Lokasi di luar wilayah layanan (Bandung Raya)"))?;
            Ok(Some((lat, lon, cell)))
        }
        _ => Err(invalid("lat dan lon harus diisi bersama")),
    }
}

pub fn point(q: &Query) -> Result<(f64, f64, CellId), ApiError> {
    point_opt(q)?.ok_or_else(|| invalid("lat dan lon wajib diisi"))
}

pub fn bbox(q: &Query) -> Result<Option<Bbox>, ApiError> {
    let Some(raw) = q.get("bbox") else {
        return Ok(None);
    };
    let parts: Vec<f64> = raw
        .split(',')
        .map(|p| p.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| invalid("bbox harus min_lon,min_lat,max_lon,max_lat"))?;
    let [min_lon, min_lat, max_lon, max_lat] = parts[..] else {
        return Err(invalid("bbox harus min_lon,min_lat,max_lon,max_lat"));
    };
    if !(min_lon < max_lon && min_lat < max_lat) || parts.iter().any(|v| !v.is_finite()) {
        return Err(invalid("bbox tidak valid: min harus lebih kecil dari max"));
    }
    Ok(Some(Bbox {
        min_lon,
        min_lat,
        max_lon,
        max_lat,
    }))
}

pub fn limit_offset(q: &Query) -> Result<(usize, usize), ApiError> {
    let limit = int_opt(q, "limit", 1, 200)?.unwrap_or(50) as usize;
    let offset = int_opt(q, "offset", 0, i64::MAX)?.unwrap_or(0) as usize;
    Ok((limit, offset))
}

pub fn time_opt(q: &Query, name: &str) -> Result<Option<DateTime<Utc>>, ApiError> {
    match q.get(name) {
        None => Ok(None),
        Some(v) => DateTime::parse_from_rfc3339(v)
            .map(|t| Some(t.with_timezone(&Utc)))
            .map_err(|_| {
                invalid(format!(
                    "{name} harus waktu ISO 8601, contoh 2026-11-20T10:15:03Z"
                ))
            }),
    }
}
