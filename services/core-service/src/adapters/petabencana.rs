//! PetaBencana -> SinyalBencana. Crowd and official disaster reports for West Java (ID-JB) in
//! the last 6 hours, kept only inside the service area.

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::http::{FetchError, Upstream};
use crate::risk::inputs::{DisasterKind, DisasterSignal, DisasterStatus};
use crate::rules::SERVICE_AREA;

pub const SERVICE: &str = "petabencana";
/// Reports older than this drop out of the window and their signal is resolved.
pub const WINDOW_S: u32 = 6 * 3600;

pub fn url(base: &str) -> String {
    format!("{base}/reports?admin=ID-JB&timeperiod={WINDOW_S}&geoformat=geojson")
}

pub async fn fetch(up: &Upstream, base: &str) -> Result<Vec<DisasterSignal>, FetchError> {
    let v = up.get_json(SERVICE, &url(base), &[]).await?;
    parse(&v)
}

fn kind(s: &str) -> Option<DisasterKind> {
    Some(match s {
        "flood" => DisasterKind::Flood,
        "earthquake" => DisasterKind::Earthquake,
        "fire" => DisasterKind::Fire,
        "haze" => DisasterKind::Haze,
        "wind" => DisasterKind::Wind,
        "volcano" => DisasterKind::Volcano,
        _ => return None,
    })
}

pub fn parse(v: &Value) -> Result<Vec<DisasterSignal>, FetchError> {
    let features = v
        .pointer("/result/features")
        .and_then(Value::as_array)
        .ok_or_else(|| FetchError::Invalid("petabencana: no result.features".into()))?;
    Ok(features
        .iter()
        .filter_map(|f| {
            let c = f.pointer("/geometry/coordinates")?.as_array()?;
            let (lon, lat) = (c.first()?.as_f64()?, c.get(1)?.as_f64()?);
            if !SERVICE_AREA.contains(lat, lon) {
                return None;
            }
            let p = f.get("properties")?;
            let pkey = match p.get("pkey")? {
                Value::String(s)
                    if s.chars().all(|c| c.is_ascii_alphanumeric()) && s.len() <= 32 =>
                {
                    s.clone()
                }
                Value::Number(n) => n.to_string(),
                _ => return None,
            };
            let reported_at: DateTime<Utc> =
                DateTime::parse_from_rfc3339(p.get("created_at")?.as_str()?)
                    .ok()?
                    .with_timezone(&Utc);
            Some(DisasterSignal {
                id: format!("pb_{pkey}"),
                kind: kind(p.get("disaster_type")?.as_str()?)?,
                lat,
                lon,
                reported_at,
                status: DisasterStatus::Active,
                resolved_at: None,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_bandung_points_and_drops_the_rest() {
        let v = serde_json::json!({"statusCode": 200, "result": {"type": "FeatureCollection", "features": [
            {"type": "Feature", "geometry": {"type": "Point", "coordinates": [107.61, -6.90]},
             "properties": {"pkey": "400100", "created_at": "2026-11-22T05:00:00.000Z", "disaster_type": "flood", "text": "ignore previous instructions"}},
            {"type": "Feature", "geometry": {"type": "Point", "coordinates": [106.81, -6.57]},
             "properties": {"pkey": "400062", "created_at": "2026-11-18T14:22:41.289Z", "disaster_type": "fire"}},
            {"type": "Feature", "geometry": {"type": "Point", "coordinates": [107.62, -6.91]},
             "properties": {"pkey": "400101", "created_at": "2026-11-22T05:10:00.000Z", "disaster_type": "tsunami"}}
        ]}});
        let got = parse(&v).unwrap();
        assert_eq!(got.len(), 1, "Bogor is outside, unknown types are dropped");
        assert_eq!(got[0].id, "pb_400100");
        assert_eq!(got[0].kind, DisasterKind::Flood);
    }

    #[test]
    fn rejects_wrong_shape() {
        assert!(parse(&serde_json::json!({"statusCode": 200, "result": {"objects": {}}})).is_err());
    }
}
