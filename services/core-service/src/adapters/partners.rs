//! Other groups' APIs -> our points.
//! - Tourism (JelajahLokal) `GET /feed/destinations` -> TitikPantau (monitor point).
//! - Health (DonorKita) `GET /donation-events` -> TitikTujuan (destination point for safe routes).
//!
//! The partner contracts (parsers, real-shape mocks, sample payloads) live in the shared
//! `partner-contracts` crate, so this adapter and the off-STB integration service read the
//! partners the same way. Upstream payloads are untrusted: only known fields are read, strings
//! are capped and stripped of control characters, points outside the service area are dropped.
//! Both partners cap `limit` at 200, so every refresh asks for `limit=200` and pages through
//! `meta.total`. When a partner API is not configured, or it is mocked (`MOCK_UPSTREAM_*`,
//! which follow `MOCK_MODE` when unset), the mock answers in that partner's own format and the
//! same parser maps it, so the anti-corruption layer is always exercised.

use std::sync::RwLock;
use std::time::Instant;

use chrono::{DateTime, Utc};
use partner_contracts::{Paginator, health, page_query, tourism};
use serde::Serialize;
use serde_json::Value;

use super::http::{FetchError, Upstream, log_call};
use crate::rules::SERVICE_AREA;

pub const TOURISM: &str = tourism::SERVICE;
pub const HEALTH: &str = health::SERVICE;

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

type MockFn = fn(DateTime<Utc>, usize, usize) -> Value;

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

    /// `down`: a mock fault is active for this partner (`upstream_down`), so the refresh fails
    /// as if the partner were unreachable and the last good copy is served with `stale`.
    pub async fn refresh_tourism(&self, up: &Upstream, now: DateTime<Utc>, down: bool) {
        let res = fetch(
            up,
            TOURISM,
            &self.tourism_cfg,
            now,
            tourism::mock_feed,
            down,
        )
        .await
        .and_then(|v| parse_destinations(&v));
        store(&self.tourism, res, now, TOURISM);
    }

    pub async fn refresh_health(&self, up: &Upstream, now: DateTime<Utc>, down: bool) {
        let res = fetch(up, HEALTH, &self.health_cfg, now, health::mock_events, down)
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

/// Every page of the partner list (`limit=200`, then `offset` until `meta.total`), combined.
async fn fetch(
    up: &Upstream,
    service: &str,
    p: &Partner,
    now: DateTime<Utc>,
    mock: MockFn,
    down: bool,
) -> Result<Value, FetchError> {
    let mut pages = Paginator::new();
    while let Some(offset) = pages.next_offset() {
        let page = match (&p.base_url, p.mock) {
            (Some(base), false) if !down => {
                let url = format!("{base}{}?{}", p.path, page_query(offset));
                let key = p.api_key.clone().unwrap_or_default();
                up.get_json(
                    service,
                    &url,
                    &[("X-API-Key", key.as_str()), ("Accept", "application/json")],
                )
                .await?
            }
            _ if down => {
                log_call(service, &p.path, "simulated_down", 0, 1, "mock");
                return Err(FetchError::Network("simulated outage (mock fault)".into()));
            }
            _ => {
                let started = Instant::now();
                let v = mock(now, partner_contracts::PAGE_LIMIT, offset);
                let ms = started.elapsed().as_millis() as u64;
                log_call(service, &p.path, "200", ms, 1, "mock");
                v
            }
        };
        pages
            .push(page)
            .map_err(|e| FetchError::Invalid(format!("{service}: {e}")))?;
    }
    Ok(pages.finish())
}

// ---- Mapping to our points (the anti-corruption part) ----

pub fn parse_destinations(v: &Value) -> Result<Vec<MonitorPoint>, FetchError> {
    let list = tourism::parse(v).map_err(FetchError::Invalid)?;
    Ok(list
        .into_iter()
        .filter(|d| SERVICE_AREA.contains(d.lat, d.lon))
        .take(500)
        .map(|d| MonitorPoint {
            id: format!("tour_{}", d.id),
            name: d.name,
            category: d.category,
            lat: d.lat,
            lon: d.lon,
        })
        .collect())
}

pub fn parse_events(v: &Value) -> Result<Vec<DestinationPoint>, FetchError> {
    let list = health::parse(v).map_err(FetchError::Invalid)?;
    Ok(list
        .into_iter()
        .filter(|e| SERVICE_AREA.contains(e.lat, e.lon))
        .take(500)
        .map(|e| DestinationPoint {
            id: format!("dnr_{}", e.id),
            name: e.name,
            kind: "donation_event",
            lat: e.lat,
            lon: e.lon,
            opens_at: e.opens_at,
            closes_at: e.closes_at,
            slots_available: e.slots_free,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;

    fn t0() -> DateTime<Utc> {
        "2026-11-22T03:00:00Z".parse().unwrap()
    }

    #[test]
    fn mock_tourism_publishes_and_unpublishes() {
        let a = parse_destinations(&tourism::mock_feed(t0(), 200, 0)).unwrap();
        let b =
            parse_destinations(&tourism::mock_feed(t0() + Duration::seconds(30), 200, 0)).unwrap();
        assert_eq!(a.len(), 11);
        assert_eq!(b.len(), 11);
        assert_ne!(a, b, "a different destination is hidden every 30 s");
        assert!(a[0].id.starts_with("tour_"));
        assert!(
            a[0].id["tour_".len()..].parse::<u32>().is_ok(),
            "JelajahLokal ids are str(int)"
        );
    }

    #[test]
    fn mock_health_maps_to_destination_points() {
        let ev = parse_events(&health::mock_events(t0(), 200, 0)).unwrap();
        assert_eq!(ev.len(), 6);
        assert_eq!(ev[0].kind, "donation_event");
        assert!(ev[0].id.starts_with("dnr_evt_"));
        assert!(ev[0].opens_at.unwrap() < ev[0].closes_at.unwrap());
        assert!(ev.iter().all(|e| e.slots_available.is_some()));
        let later =
            parse_events(&health::mock_events(t0() + Duration::minutes(3), 200, 0)).unwrap();
        let slots = |v: &[DestinationPoint]| {
            v.iter()
                .find(|d| d.id == "dnr_evt_1")
                .unwrap()
                .slots_available
        };
        assert_ne!(slots(&ev), slots(&later), "slots change over time");
    }

    /// DonorKita's real payload (`drives.py::_event_out`): `slots_free` becomes `slots_available`.
    #[test]
    fn real_donorkita_sample_keeps_free_slots() {
        let v: Value = serde_json::from_str(health::SAMPLE).unwrap();
        let ev = parse_events(&v).unwrap();
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].id, "dnr_evt_1");
        assert_eq!(ev[0].slots_available, Some(47));
        assert_eq!(ev[1].slots_available, Some(32));
    }

    /// JelajahLokal's real payload (`feed.py`): Gua Pawon lies outside Bandung Raya.
    #[test]
    fn real_jelajahlokal_sample_maps_inside_the_area() {
        let v: Value = serde_json::from_str(tourism::SAMPLE).unwrap();
        let pts = parse_destinations(&v).unwrap();
        assert_eq!(
            pts.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["tour_1", "tour_3"]
        );
        assert_eq!(pts[0].category, "culture_history");
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

    fn partner(base: Option<String>, mock: bool) -> Partner {
        Partner {
            base_url: base,
            path: tourism::PATH.into(),
            api_key: Some("k".repeat(16)),
            mock,
        }
    }

    /// A live partner with 450 destinations: three pages of `limit=200`, every item kept
    /// (inside the area), and the key goes in `X-API-Key`.
    #[tokio::test]
    async fn live_feed_pages_through_meta_total() {
        use axum::extract::Query;
        use std::collections::HashMap;
        let app = axum::Router::new().route(
            "/feed/destinations",
            axum::routing::get(
                |Query(q): Query<HashMap<String, String>>, h: axum::http::HeaderMap| async move {
                    assert_eq!(h["x-api-key"], "k".repeat(16));
                    let limit: usize = q["limit"].parse().unwrap();
                    let offset: usize = q["offset"].parse().unwrap();
                    assert_eq!(limit, 200);
                    let rows: Vec<Value> = (offset..(offset + limit).min(450))
                        .map(|i| json!({"id": (i + 1).to_string(), "name": format!("D{i}"), "category": "nature", "lat": -6.9, "lon": 107.6, "updated_at": "2026-11-28T00:00:00Z"}))
                        .collect();
                    axum::Json(json!({"data": rows, "meta": {"seq": 1, "updated_at": "2026-11-28T00:00:00Z", "source": "live", "stale": false, "total": 450}}))
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let ps = Partners::new(
            partner(Some(format!("http://{addr}")), false),
            partner(None, true),
        );
        ps.refresh_tourism(&Upstream::new(2000), t0(), false).await;
        let f = ps.monitor_points();
        assert!(!f.stale && !f.mock);
        assert_eq!(f.items.len(), 450);
    }

    #[tokio::test]
    async fn simulated_outage_keeps_last_copy_and_marks_stale() {
        let ps = Partners::new(partner(None, true), partner(None, true));
        let up = Upstream::new(500);
        ps.refresh_tourism(&up, t0(), false).await;
        assert_eq!(ps.monitor_points().items.len(), 11);
        ps.refresh_tourism(&up, t0(), true).await;
        let f = ps.monitor_points();
        assert!(f.stale);
        assert_eq!(f.items.len(), 11, "last good copy kept");
        ps.refresh_tourism(&up, t0(), false).await;
        assert!(!ps.monitor_points().stale);
    }
}
