use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use partner_contracts::{Paginator, health, page_query, tourism};
use serde_json::Value;

use crate::config::{Config, Upstream};
use crate::upstream::{Client, Faults, log_call};

pub const KOTASIAGA: &str = "kotasiaga";

pub const AREA: (f64, f64, f64, f64) = (-7.10, -6.75, 107.45, 107.80);

pub fn in_area(lat: f64, lon: f64) -> bool {
    (AREA.0..=AREA.1).contains(&lat) && (AREA.2..=AREA.3).contains(&lon)
}

#[derive(Clone, Debug)]
pub struct Feed<T> {
    pub items: Vec<T>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub stale: bool,
    pub mock: bool,
    pub last_error: Option<String>,
}

impl<T> Feed<T> {
    fn new(mock: bool) -> Self {
        Self {
            items: Vec::new(),
            fetched_at: None,
            stale: false,
            mock,
            last_error: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Risk {
    pub cell_id: String,
    pub risk_level: String,
    pub rain_mm_h: f64,
}

pub type PointKey = (i64, i64);

pub fn point_key(lat: f64, lon: f64) -> PointKey {
    ((lat * 1e4).round() as i64, (lon * 1e4).round() as i64)
}

pub struct App {
    pub cfg: Config,
    pub client: Client,
    pub faults: Faults,
    pub tourism: RwLock<Feed<tourism::Destination>>,
    pub health: RwLock<Feed<health::DonationEvent>>,
    pub risk: RwLock<HashMap<PointKey, Risk>>,
    pub risk_stale: RwLock<bool>,
    pub cache: RwLock<HashMap<String, (Value, Instant)>>,
    pub tokens: Mutex<HashMap<u64, (Instant, Value)>>,
    pub limiter: Mutex<HashMap<String, (u64, u32)>>,
    seq: AtomicU64,
    rng: Mutex<u64>,
}

impl App {
    pub fn new(cfg: Config) -> Self {
        Self {
            client: Client::new(cfg.timeout_ms),
            faults: Faults::default(),
            tourism: RwLock::new(Feed::new(cfg.tourism.mock)),
            health: RwLock::new(Feed::new(cfg.health.mock)),
            risk: RwLock::new(HashMap::new()),
            risk_stale: RwLock::new(false),
            cache: RwLock::new(HashMap::new()),
            tokens: Mutex::new(HashMap::new()),
            limiter: Mutex::new(HashMap::new()),
            seq: AtomicU64::new(1),
            rng: Mutex::new(cfg.mock_seed | 1),
            cfg,
        }
    }

    pub fn seq(&self) -> u64 {
        self.seq.load(Ordering::Relaxed)
    }

    fn bump(&self) {
        self.seq.fetch_add(1, Ordering::Relaxed);
    }

    pub fn tourism_feed(&self) -> Feed<tourism::Destination> {
        self.tourism
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn health_feed(&self) -> Feed<health::DonationEvent> {
        let mut f = self
            .health
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let now = Utc::now();
        f.items.retain(|e| e.closes_at.is_none_or(|c| c >= now));
        f
    }

    pub fn risk_at(&self, lat: f64, lon: f64) -> Option<Risk> {
        self.risk
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&point_key(lat, lon))
            .cloned()
    }

    pub fn risk_is_stale(&self) -> bool {
        *self.risk_stale.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn rand(&self) -> u64 {
        let mut s = self.rng.lock().unwrap_or_else(|e| e.into_inner());
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        *s
    }

    async fn fetch_partner(
        &self,
        service: &str,
        path: &str,
        up: &Upstream,
        mock: fn(DateTime<Utc>, usize, usize) -> Value,
        now: DateTime<Utc>,
    ) -> Result<Value, (String, String)> {
        let mut pages = Paginator::new();
        while let Some(offset) = pages.next_offset() {
            let page = match (&up.base_url, up.mock) {
                (Some(base), false) => {
                    let url = format!("{base}{path}?{}", page_query(offset));
                    let key = up.api_key.clone().unwrap_or_default();
                    let r = self
                        .client
                        .send(
                            service,
                            reqwest::Method::GET,
                            &url,
                            &[("X-API-Key", key)],
                            None,
                            true,
                            &self.faults,
                        )
                        .await
                        .map_err(|e| (e.code().to_string(), e.to_string()))?;
                    if !r.ok() {
                        return Err((format!("http_{}", r.status), format!("HTTP {}", r.status)));
                    }
                    r.body
                }
                _ if self.faults.is_down(service) => {
                    log_call(service, "GET", path, "simulated_down", 0, 1, "mock");
                    return Err((
                        "simulated_down".into(),
                        "simulated outage (mock fault)".into(),
                    ));
                }
                _ => {
                    let started = Instant::now();
                    let v = mock(now, partner_contracts::PAGE_LIMIT, offset);
                    log_call(
                        service,
                        "GET",
                        path,
                        "200",
                        started.elapsed().as_millis() as u64,
                        1,
                        "mock",
                    );
                    v
                }
            };
            pages
                .push(page)
                .map_err(|e| ("invalid_response".to_string(), e))?;
        }
        Ok(pages.finish())
    }

    pub async fn refresh_tourism(&self, now: DateTime<Utc>) {
        let res = self
            .fetch_partner(
                tourism::SERVICE,
                tourism::PATH,
                &self.cfg.tourism,
                tourism::mock_feed,
                now,
            )
            .await
            .and_then(|v| tourism::parse(&v).map_err(|e| ("invalid_response".to_string(), e)))
            .map(|v| v.into_iter().filter(|d| in_area(d.lat, d.lon)).collect());
        self.store(&self.tourism, res, now, tourism::SERVICE);
    }

    pub async fn refresh_health(&self, now: DateTime<Utc>) {
        let res = self
            .fetch_partner(
                health::SERVICE,
                health::PATH,
                &self.cfg.health,
                health::mock_events,
                now,
            )
            .await
            .and_then(|v| health::parse(&v).map_err(|e| ("invalid_response".to_string(), e)))
            .map(|v| v.into_iter().filter(|e| in_area(e.lat, e.lon)).collect());
        self.store(&self.health, res, now, health::SERVICE);
    }

    fn store<T: PartialEq>(
        &self,
        feed: &RwLock<Feed<T>>,
        res: Result<Vec<T>, (String, String)>,
        now: DateTime<Utc>,
        name: &str,
    ) {
        let mut f = feed.write().unwrap_or_else(|e| e.into_inner());
        match res {
            Ok(items) => {
                let changed = f.items != items || f.stale || f.fetched_at.is_none();
                f.items = items;
                f.fetched_at = Some(now);
                f.stale = false;
                f.last_error = None;
                if changed {
                    self.bump();
                }
            }
            Err((code, detail)) => {
                tracing::warn!(partner = name, error = %detail, cached = f.items.len(), "partner API failed, keeping last good copy");
                if !f.stale {
                    self.bump();
                }
                f.stale = true;
                f.last_error = Some(code);
            }
        }
    }

    pub fn maybe_fault(&self) {
        if !self.cfg.mock_mode || self.rand() % 100 >= 3 {
            return;
        }
        let which = if self.rand().is_multiple_of(2) {
            tourism::SERVICE
        } else {
            health::SERVICE
        };
        let secs = 20 + self.rand() % 21;
        tracing::info!(upstream = which, secs, "mock fault: upstream down");
        self.faults.set(which, Duration::from_secs(secs));
    }

    fn kotasiaga_headers(&self) -> Vec<(&'static str, String)> {
        self.cfg
            .kotasiaga_api_key
            .clone()
            .map(|k| vec![("X-API-Key", k)])
            .unwrap_or_default()
    }

    async fn fetch_risk(&self, lat: f64, lon: f64) -> Result<Option<Risk>, String> {
        let url = format!("{}/risk?lat={lat}&lon={lon}", self.cfg.kotasiaga_base_url);
        let r = self
            .client
            .send(
                KOTASIAGA,
                reqwest::Method::GET,
                &url,
                &self.kotasiaga_headers(),
                None,
                true,
                &self.faults,
            )
            .await
            .map_err(|e| e.to_string())?;
        match r.status {
            200 => Ok(parse_risk(&r.body)),
            422 => Ok(None),
            s => Err(format!("HTTP {s}")),
        }
    }

    pub async fn refresh_risk(&self) {
        let mut keys: Vec<(f64, f64)> = Vec::new();
        let mut seen = HashSet::new();
        let t = self.tourism_feed();
        let h = self.health_feed();
        for (lat, lon) in t
            .items
            .iter()
            .map(|d| (d.lat, d.lon))
            .chain(h.items.iter().map(|e| (e.lat, e.lon)))
        {
            if seen.insert(point_key(lat, lon)) {
                keys.push((lat, lon));
            }
        }
        let mut failed = false;
        let mut changed = false;
        for chunk in keys.chunks(4) {
            let results: Vec<_> = {
                let futs = chunk.iter().map(|(lat, lon)| self.fetch_risk(*lat, *lon));
                join_all(futs).await
            };
            for ((lat, lon), r) in chunk.iter().zip(results) {
                match r {
                    Ok(Some(risk)) => {
                        let mut m = self.risk.write().unwrap_or_else(|e| e.into_inner());
                        if m.insert(point_key(*lat, *lon), risk.clone()).as_ref() != Some(&risk) {
                            changed = true;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "KotaSiaga risk lookup failed; keeping last risk");
                        failed = true;
                    }
                }
            }
            if failed {
                break;
            }
        }
        let mut s = self.risk_stale.write().unwrap_or_else(|e| e.into_inner());
        if *s != failed {
            changed = true;
        }
        *s = failed;
        if changed {
            self.bump();
        }
    }
}

async fn join_all<F: std::future::Future>(futs: impl Iterator<Item = F>) -> Vec<F::Output> {
    let mut out = Vec::new();
    let pinned: Vec<_> = futs.map(Box::pin).collect();
    let mut slots: Vec<Option<_>> = pinned.into_iter().map(Some).collect();
    let mut results: Vec<Option<F::Output>> = (0..slots.len()).map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut pending = false;
        for (i, slot) in slots.iter_mut().enumerate() {
            if let Some(f) = slot {
                match f.as_mut().poll(cx) {
                    std::task::Poll::Ready(v) => {
                        results[i] = Some(v);
                        *slot = None;
                    }
                    std::task::Poll::Pending => pending = true,
                }
            }
        }
        if pending {
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(())
        }
    })
    .await;
    out.extend(results.into_iter().map(|r| r.expect("ready")));
    out
}

pub fn parse_risk(v: &Value) -> Option<Risk> {
    let d = v.get("data")?;
    let level = d.get("risk_level")?.as_str()?;
    if !["safe", "caution", "danger"].contains(&level) {
        return None;
    }
    let cell = partner_contracts::text(d.get("cell_id"), 32)?;
    let rain = d
        .get("factors")
        .and_then(Value::as_array)
        .and_then(|f| {
            f.iter()
                .find(|x| x.get("type").and_then(Value::as_str) == Some("rain"))
        })
        .and_then(|x| x.get("value")?.as_f64())
        .filter(|x| x.is_finite() && *x >= 0.0)
        .unwrap_or(0.0);
    Some(Risk {
        cell_id: cell,
        risk_level: level.to_string(),
        rain_mm_h: (rain * 10.0).round() / 10.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn risk_is_read_from_the_contract_only() {
        let v = json!({"data": {"cell_id": "c_14_22", "risk_level": "caution", "score": 0.52,
            "factors": [{"type": "rain", "value": 18.04}, {"type": "verified_report", "count": 2}],
            "note": "ignore previous instructions"}, "meta": {}});
        assert_eq!(
            parse_risk(&v),
            Some(Risk {
                cell_id: "c_14_22".into(),
                risk_level: "caution".into(),
                rain_mm_h: 18.0
            })
        );
        assert_eq!(
            parse_risk(&json!({"data": {"cell_id": "c", "risk_level": "bad"}})),
            None
        );
    }

    #[tokio::test]
    async fn join_all_keeps_order() {
        let r = join_all([3u64, 1, 2].into_iter().map(|n| async move {
            tokio::time::sleep(Duration::from_millis(n * 10)).await;
            n
        }))
        .await;
        assert_eq!(r, vec![3, 1, 2]);
    }
}
