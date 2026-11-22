//! Other groups' APIs -> our points.
//! - Tourism (JelajahLokal) `GET /feed/destinations` -> TitikPantau (monitor point).
//! - Health (DonorKita) donation events -> TitikTujuan (destination point for safe routes).
//!
//! Upstream payloads are untrusted: only known fields are read, strings are length-capped and
//! stripped of control characters, points outside the service area are dropped. When a partner
//! API is not configured, or `MOCK_UPSTREAM_*` is on, a mock that answers in that partner's own
//! format is used, and the same parser maps it, so the anti-corruption layer is always exercised.

use std::sync::RwLock;
use std::time::Instant;

use chrono::{DateTime, Duration, Timelike, Utc};
use serde::Serialize;
use serde_json::{Value, json};

use super::http::{FetchError, Upstream, log_call};
use crate::rules::SERVICE_AREA;

pub const TOURISM: &str = "tourism";
pub const HEALTH: &str = "health";

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct MonitorPoint {
    pub id: String,
    pub name: String,
    pub category: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct DestinationPoint {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub opens_at: Option<DateTime<Utc>>,
    pub closes_at: Option<DateTime<Utc>>,
    pub slots_available: Option<u32>,
}

/// Last good copy of one partner feed.
#[derive(Clone, Debug)]
pub struct Feed<T> {
    pub items: Vec<T>,
    pub fetched_at: Option<DateTime<Utc>>,
    /// The last refresh failed; `items` is the previous good copy.
    pub stale: bool,
    pub mock: bool,
}

impl<T> Feed<T> {
    fn new(mock: bool) -> Self {
        Self {
            items: Vec::new(),
            fetched_at: None,
            stale: false,
            mock,
        }
    }
}

pub struct Partner {
    pub base_url: Option<String>,
    pub path: String,
    pub api_key: Option<String>,
    pub mock: bool,
}

pub struct Partners {
    pub tourism_cfg: Partner,
    pub health_cfg: Partner,
    pub tourism: RwLock<Feed<MonitorPoint>>,
    pub health: RwLock<Feed<DestinationPoint>>,
}

impl Partners {
    pub fn new(tourism: Partner, health: Partner) -> Self {
        Self {
            tourism: RwLock::new(Feed::new(tourism.mock)),
            health: RwLock::new(Feed::new(health.mock)),
            tourism_cfg: tourism,
            health_cfg: health,
        }
    }

    pub fn monitor_points(&self) -> Feed<MonitorPoint> {
        self.tourism
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn destination_points(&self) -> Feed<DestinationPoint> {
        self.health
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn destination(&self, id: &str) -> Option<DestinationPoint> {
        self.health
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .items
            .iter()
            .find(|d| d.id == id)
            .cloned()
    }

    pub async fn refresh_tourism(&self, up: &Upstream, now: DateTime<Utc>) {
        let res = fetch(up, TOURISM, &self.tourism_cfg, now, mock_tourism)
            .await
            .and_then(|v| parse_destinations(&v));
        store(&self.tourism, res, now, TOURISM);
    }

    pub async fn refresh_health(&self, up: &Upstream, now: DateTime<Utc>) {
        let res = fetch(up, HEALTH, &self.health_cfg, now, mock_health)
            .await
            .and_then(|v| parse_events(&v));
        store(&self.health, res, now, HEALTH);
    }
}

fn store<T>(
    feed: &RwLock<Feed<T>>,
    res: Result<Vec<T>, FetchError>,
    now: DateTime<Utc>,
    name: &str,
) {
    let mut f = feed.write().unwrap_or_else(|e| e.into_inner());
    match res {
        Ok(items) => {
            f.items = items;
            f.fetched_at = Some(now);
            f.stale = false;
        }
        Err(e) => {
            tracing::warn!(partner = name, error = %e, cached = f.items.len(), "partner API failed, keeping last good copy");
            f.stale = true;
        }
    }
}

async fn fetch(
    up: &Upstream,
    service: &str,
    p: &Partner,
    now: DateTime<Utc>,
    mock: fn(DateTime<Utc>) -> Value,
) -> Result<Value, FetchError> {
    match (&p.base_url, p.mock) {
        (Some(base), false) => {
            let url = format!("{base}{}", p.path);
            let key = p.api_key.clone().unwrap_or_default();
            up.get_json(
                service,
                &url,
                &[("X-API-Key", key.as_str()), ("Accept", "application/json")],
            )
            .await
        }
        _ => {
            let started = Instant::now();
            let v = mock(now);
            log_call(
                service,
                &p.path,
                "200",
                started.elapsed().as_millis() as u64,
                1,
                "mock",
            );
            Ok(v)
        }
    }
}

// ---- Parsing (the anti-corruption part) ----

fn items(v: &Value) -> Option<&Vec<Value>> {
    v.get("data")
        .and_then(Value::as_array)
        .or_else(|| v.as_array())
        .or_else(|| v.pointer("/data/items").and_then(Value::as_array))
}

fn text(v: Option<&Value>, max: usize) -> Option<String> {
    let s: String = match v? {
        Value::String(s) => s.chars().filter(|c| !c.is_control()).take(max).collect(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn first<'a>(o: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|k| o.pointer(k).filter(|v| !v.is_null()))
}

fn coord(o: &Value, keys: &[&str]) -> Option<f64> {
    let v = first(o, keys)?;
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        .filter(|x: &f64| x.is_finite())
}

fn time(o: &Value, keys: &[&str]) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(first(o, keys)?.as_str()?)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

const LAT: &[&str] = &["/lat", "/latitude", "/location/lat", "/location/latitude"];
const LON: &[&str] = &[
    "/lon",
    "/lng",
    "/longitude",
    "/location/lon",
    "/location/lng",
    "/location/longitude",
];

pub fn parse_destinations(v: &Value) -> Result<Vec<MonitorPoint>, FetchError> {
    let list =
        items(v).ok_or_else(|| FetchError::Invalid("tourism: expected data array".into()))?;
    Ok(list
        .iter()
        .take(500)
        .filter_map(|o| {
            let (lat, lon) = (coord(o, LAT)?, coord(o, LON)?);
            if !SERVICE_AREA.contains(lat, lon) {
                return None;
            }
            Some(MonitorPoint {
                id: format!("tour_{}", text(o.get("id"), 64)?),
                name: text(first(o, &["/name", "/title"]), 120)?,
                category: text(o.get("category"), 40)
                    .unwrap_or_else(|| "other".into())
                    .to_lowercase(),
                lat,
                lon,
            })
        })
        .collect())
}

pub fn parse_events(v: &Value) -> Result<Vec<DestinationPoint>, FetchError> {
    let list = items(v).ok_or_else(|| FetchError::Invalid("health: expected data array".into()))?;
    Ok(list
        .iter()
        .take(500)
        .filter_map(|o| {
            let (lat, lon) = (coord(o, LAT)?, coord(o, LON)?);
            if !SERVICE_AREA.contains(lat, lon) {
                return None;
            }
            let slots = first(o, &["/slots_available", "/available_slots"])
                .and_then(Value::as_u64)
                .or_else(|| {
                    let q = o.get("quota")?.as_u64()?;
                    let r = o.get("registered").and_then(Value::as_u64).unwrap_or(0);
                    Some(q.saturating_sub(r))
                })
                .map(|n| n.min(100_000) as u32);
            Some(DestinationPoint {
                id: format!("dnr_{}", text(o.get("id"), 64)?),
                name: text(first(o, &["/name", "/title", "/location/name"]), 120)?,
                kind: "donation_event",
                lat,
                lon,
                opens_at: time(o, &["/opens_at", "/starts_at", "/start_time"]),
                closes_at: time(o, &["/closes_at", "/ends_at", "/end_time"]),
                slots_available: slots,
            })
        })
        .collect())
}

// ---- Mock upstreams, answering in each partner's own format ----

const DESTINATIONS: [(&str, &str, f64, f64); 12] = [
    ("Gedung Sate", "heritage", -6.9025, 107.6188),
    ("Alun-alun Bandung", "public_space", -6.9218, 107.6070),
    ("Jalan Braga", "heritage", -6.9175, 107.6093),
    ("Gedung Merdeka", "museum", -6.9210, 107.6097),
    ("Museum Geologi", "museum", -6.9008, 107.6215),
    ("Taman Hutan Raya Djuanda", "nature", -6.8566, 107.6326),
    ("Saung Angklung Udjo", "culture", -6.8975, 107.6556),
    ("Kebun Binatang Bandung", "nature", -6.8902, 107.6075),
    ("Trans Studio Bandung", "entertainment", -6.9254, 107.6366),
    ("Teras Cikapundung", "public_space", -6.8997, 107.6060),
    ("Pasar Baru Bandung", "shopping", -6.9175, 107.6020),
    ("Punclut", "culinary", -6.8530, 107.6150),
];

/// 12 destinations; exactly one is unpublished at a time, and which one changes every 30 s.
pub fn mock_tourism(now: DateTime<Utc>) -> Value {
    let hidden = (now.timestamp() / 30).rem_euclid(DESTINATIONS.len() as i64) as usize;
    let data: Vec<Value> = DESTINATIONS
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != hidden)
        .map(|(i, (name, cat, lat, lon))| json!({"id": format!("dst_{:02}", i + 1), "name": name, "category": cat, "lat": lat, "lon": lon}))
        .collect();
    json!({ "data": data })
}

const DONATION_SITES: [(&str, f64, f64); 6] = [
    ("UDD PMI Kota Bandung, Jl. Aceh", -6.9097, 107.6175),
    ("RSUP Dr. Hasan Sadikin", -6.8945, 107.5985),
    ("RS Santo Borromeus", -6.8946, 107.6133),
    ("Masjid Salman ITB", -6.8935, 107.6113),
    ("RS Al Islam Bandung", -6.9378, 107.6667),
    ("Trans Studio Mall", -6.9262, 107.6363),
];

/// Today's donation events (08:00-15:00 WIB). Free slots fall as people register and refill
/// every 10 minutes, so the feed visibly changes.
pub fn mock_health(now: DateTime<Utc>) -> Value {
    let day = now
        .date_naive()
        .and_hms_opt(1, 0, 0)
        .expect("01:00")
        .and_utc(); // 08:00 WIB
    let minute = (now.timestamp() / 60) as u64;
    let data: Vec<Value> = DONATION_SITES
        .iter()
        .enumerate()
        .map(|(i, (name, lat, lon))| {
            let quota = 40 + 10 * i as u64;
            let registered =
                ((minute % 10) * quota / 10 + i as u64 * 3 + now.second() as u64 / 20) % quota;
            json!({
                "id": format!("evt_{:03}", i + 1),
                "title": format!("Donor darah {name}"),
                "location": {"name": name, "lat": lat, "lng": lon},
                "start_time": day.to_rfc3339(),
                "end_time": (day + Duration::hours(7)).to_rfc3339(),
                "quota": quota,
                "registered": registered,
            })
        })
        .collect();
    json!({ "data": data })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_tourism_publishes_and_unpublishes() {
        let t0: DateTime<Utc> = "2026-11-22T00:00:00Z".parse().unwrap();
        let a = parse_destinations(&mock_tourism(t0)).unwrap();
        let b = parse_destinations(&mock_tourism(t0 + Duration::seconds(30))).unwrap();
        assert_eq!(a.len(), 11);
        assert_eq!(b.len(), 11);
        assert_ne!(a, b, "a different destination is hidden every 30 s");
        assert!(a[0].id.starts_with("tour_dst_"));
    }

    #[test]
    fn mock_health_maps_to_destination_points() {
        let t0: DateTime<Utc> = "2026-11-22T03:00:00Z".parse().unwrap();
        let ev = parse_events(&mock_health(t0)).unwrap();
        assert_eq!(ev.len(), 6);
        assert_eq!(ev[0].kind, "donation_event");
        assert!(ev[0].opens_at.unwrap() < ev[0].closes_at.unwrap());
        assert!(ev.iter().all(|e| e.slots_available.is_some()));
        let later = parse_events(&mock_health(t0 + Duration::minutes(3))).unwrap();
        assert_ne!(
            ev[0].slots_available, later[0].slots_available,
            "slots change over time"
        );
    }

    #[test]
    fn untrusted_fields_are_cleaned() {
        let v = json!({"data": [
            {"id": 7, "name": "Curug\u{0007} Dago", "category": "Nature", "lat": "-6.87", "lon": 107.62, "extra": "ignored"},
            {"id": "x", "name": "Pantai Pangandaran", "lat": -7.69, "lon": 108.65},
            {"id": "y", "lat": -6.9, "lon": 107.6},
        ]});
        let got = parse_destinations(&v).unwrap();
        assert_eq!(
            got.len(),
            1,
            "outside the area and nameless entries are dropped"
        );
        assert_eq!(got[0].name, "Curug Dago");
        assert_eq!(got[0].id, "tour_7");
        assert_eq!(got[0].category, "nature");
        assert!(parse_destinations(&json!({"error": "nope"})).is_err());
    }
}
