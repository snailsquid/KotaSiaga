//! Contracts of the partner APIs KotaSiaga consumes, in one place for every adapter:
//! - Tourism, JelajahLokal (`elnoraam/jelajah-lokal`, `core-service/app/modules/feed.py`):
//!   `GET /feed/destinations` with `X-API-Key`.
//! - Health, DonorKita (`rnfaizi/DonorKita`, `core-service/app/modules/drives.py`):
//!   `GET /donation-events` with `X-API-Key`.
//!
//! Each module has a tolerant parser (the anti-corruption part: only known fields are read,
//! strings are capped and stripped of control characters, invalid items are dropped), a mock
//! that answers in exactly the partner's shape (fields, id format, enums, envelope, `limit` /
//! `offset` / `meta.total`), and the real sample payload used by the contract tests.
//! Both partners cap `limit` at 200, so [`Paginator`] pages through `meta.total`.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::{Value, json};

/// Largest page both partners accept (`limit` max 200, default 50).
pub const PAGE_LIMIT: usize = 200;
/// At most this many pages per refresh (1000 items), so a broken `meta.total` cannot loop.
pub const MAX_PAGES: usize = 5;
/// Items kept per feed after parsing.
pub const MAX_ITEMS: usize = PAGE_LIMIT * MAX_PAGES;

/// ISO 8601 UTC with `Z`, second precision: the format both partners emit.
pub fn iso(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// `?limit=200&offset=N` for one page.
pub fn page_query(offset: usize) -> String {
    format!("limit={PAGE_LIMIT}&offset={offset}")
}

/// Collects every page of a partner list: ask for `next_offset()`, fetch it, `push` the body.
/// Stops when a page is short, when `meta.total` is reached, or after [`MAX_PAGES`].
#[derive(Default)]
pub struct Paginator {
    items: Vec<Value>,
    total: Option<u64>,
    pages: usize,
    done: bool,
    meta: Option<Value>,
}

impl Paginator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_offset(&self) -> Option<usize> {
        (!self.done && self.pages < MAX_PAGES).then_some(self.items.len())
    }

    /// Adds one page. Errors when the body is not the partner's `{data: [...]}` envelope.
    pub fn push(&mut self, page: Value) -> Result<(), String> {
        let list = data_items(&page).ok_or("expected a data array")?;
        let n = list.len();
        self.items.extend(list.iter().cloned());
        self.pages += 1;
        if let Some(t) = page.pointer("/meta/total").and_then(Value::as_u64) {
            self.total = Some(t);
        }
        if self.meta.is_none() {
            self.meta = page.get("meta").cloned();
        }
        let reached = self.total.is_some_and(|t| self.items.len() as u64 >= t);
        if n < PAGE_LIMIT || n == 0 || reached {
            self.done = true;
        }
        Ok(())
    }

    pub fn pages(&self) -> usize {
        self.pages
    }

    /// One combined body, `{data: [all items], meta: {first page's meta, total}}`.
    pub fn finish(self) -> Value {
        let mut meta = self.meta.unwrap_or_else(|| json!({}));
        if let (Some(t), Some(m)) = (self.total, meta.as_object_mut()) {
            m.insert("total".into(), json!(t));
        }
        json!({ "data": self.items, "meta": meta })
    }
}

// ---- Sanitizing helpers ----

pub fn data_items(v: &Value) -> Option<&Vec<Value>> {
    v.get("data")
        .and_then(Value::as_array)
        .or_else(|| v.as_array())
        .or_else(|| v.pointer("/data/items").and_then(Value::as_array))
}

/// A string (or number) field, control characters removed, trimmed, at most `max` chars.
pub fn text(v: Option<&Value>, max: usize) -> Option<String> {
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

fn coord(o: &Value, keys: &[&str], lo: f64, hi: f64) -> Option<f64> {
    let v = first(o, keys)?;
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        .filter(|x: &f64| x.is_finite() && (lo..=hi).contains(x))
}

fn time(o: &Value, keys: &[&str]) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(first(o, keys)?.as_str()?)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

fn count(o: &Value, keys: &[&str]) -> Option<u32> {
    first(o, keys)
        .and_then(Value::as_u64)
        .map(|n| n.min(1_000_000) as u32)
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

fn lat_lon(o: &Value) -> Option<(f64, f64)> {
    Some((coord(o, LAT, -90.0, 90.0)?, coord(o, LON, -180.0, 180.0)?))
}

fn mock_meta(now: DateTime<Utc>, seq: i64, total: usize) -> Value {
    json!({"seq": seq, "updated_at": iso(now), "source": "mock", "stale": false, "total": total})
}

fn page_of<T: Clone>(rows: &[T], limit: usize, offset: usize) -> Vec<T> {
    rows.iter()
        .skip(offset)
        .take(limit.clamp(1, PAGE_LIMIT))
        .cloned()
        .collect()
}

// ---- Tourism: JelajahLokal ----

pub mod tourism {
    use super::*;

    pub const SERVICE: &str = "tourism";
    pub const PATH: &str = "/feed/destinations";
    /// JelajahLokal `CATEGORY_API` values.
    pub const CATEGORIES: [&str; 5] = [
        "nature",
        "culture_history",
        "local_culinary",
        "tourism_village",
        "craft",
    ];
    /// A real `GET /feed/destinations` body, built from JelajahLokal's `feed.py` and seed data.
    pub const SAMPLE: &str = include_str!("../fixtures/jelajahlokal_feed_destinations.json");

    /// One published destination, as JelajahLokal sends it (`id` is their opaque string).
    #[derive(Clone, Debug, PartialEq, Serialize)]
    pub struct Destination {
        pub id: String,
        pub name: String,
        pub category: String,
        pub lat: f64,
        pub lon: f64,
        pub updated_at: Option<DateTime<Utc>>,
    }

    pub fn parse(v: &Value) -> Result<Vec<Destination>, String> {
        let list = data_items(v).ok_or("tourism: expected a data array")?;
        Ok(list
            .iter()
            .take(MAX_ITEMS)
            .filter_map(|o| {
                let (lat, lon) = lat_lon(o)?;
                Some(Destination {
                    id: text(o.get("id"), 64)?,
                    name: text(first(o, &["/name", "/title"]), 120)?,
                    category: text(o.get("category"), 40)
                        .unwrap_or_else(|| "other".into())
                        .to_lowercase(),
                    lat,
                    lon,
                    updated_at: time(o, &["/updated_at"]),
                })
            })
            .collect())
    }

    const PLACES: [(&str, &str, f64, f64); 12] = [
        ("Gedung Sate", "culture_history", -6.9025, 107.6188),
        ("Jalan Braga", "culture_history", -6.9175, 107.6093),
        ("Museum Geologi", "culture_history", -6.9008, 107.6215),
        ("Taman Hutan Raya Djuanda", "nature", -6.8566, 107.6326),
        ("Saung Angklung Udjo", "craft", -6.8975, 107.6556),
        (
            "Kampung Adat Cireundeu",
            "culture_history",
            -6.9497,
            107.5375,
        ),
        ("Curug Cimahi", "nature", -6.8110, 107.5560),
        ("Punclut", "local_culinary", -6.8530, 107.6150),
        (
            "Kampung Kreatif Dago Pojok",
            "tourism_village",
            -6.8775,
            107.6230,
        ),
        ("Sentra Kaos Suci", "craft", -6.8990, 107.6370),
        (
            "Kampung Wisata Cibunut",
            "tourism_village",
            -6.9135,
            107.6155,
        ),
        (
            "Kawasan Kuliner Cibadak",
            "local_culinary",
            -6.9200,
            107.6010,
        ),
    ];

    /// `GET /feed/destinations?limit&offset` in JelajahLokal's exact shape: 12 Bandung places,
    /// exactly one unpublished at a time, a different one every 30 s ("publish or unpublish
    /// every ~30 s", AGENTS.md section 7).
    pub fn mock_feed(now: DateTime<Utc>, limit: usize, offset: usize) -> Value {
        let window = now.timestamp().div_euclid(30);
        let hidden = window.rem_euclid(PLACES.len() as i64) as usize;
        let back = (hidden + PLACES.len() - 1) % PLACES.len();
        let day = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight")
            .and_utc();
        let window_start = DateTime::from_timestamp(window * 30, 0).unwrap_or(now);
        let rows: Vec<Value> = PLACES
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != hidden)
            .map(|(i, (name, cat, lat, lon))| {
                let updated = if i == back { window_start } else { day };
                json!({"id": (i + 1).to_string(), "name": name, "category": cat,
                       "lat": lat, "lon": lon, "updated_at": iso(updated)})
            })
            .collect();
        json!({"data": page_of(&rows, limit, offset), "meta": mock_meta(now, window, rows.len())})
    }
}

// ---- Health: DonorKita ----

pub mod health {
    use super::*;

    pub const SERVICE: &str = "health";
    pub const PATH: &str = "/donation-events";
    /// A real `GET /donation-events` body, built from DonorKita's `drives.py::_event_out` and seed.
    pub const SAMPLE: &str = include_str!("../fixtures/donorkita_donation_events.json");

    /// One scheduled donation event, as DonorKita sends it.
    #[derive(Clone, Debug, PartialEq, Serialize)]
    pub struct DonationEvent {
        pub id: String,
        pub name: String,
        pub organizer: Option<String>,
        pub facility_id: Option<String>,
        pub address: Option<String>,
        pub lat: f64,
        pub lon: f64,
        pub opens_at: Option<DateTime<Utc>>,
        pub closes_at: Option<DateTime<Utc>>,
        pub slots_total: Option<u32>,
        /// DonorKita's `slots_free` (older drafts: `slots_available`, or `quota - registered`).
        pub slots_free: Option<u32>,
        pub capacity_per_hour: Option<u32>,
        pub status: Option<String>,
        pub needed_blood_types: Vec<String>,
        pub updated_at: Option<DateTime<Utc>>,
    }

    pub fn parse(v: &Value) -> Result<Vec<DonationEvent>, String> {
        let list = data_items(v).ok_or("health: expected a data array")?;
        Ok(list
            .iter()
            .take(MAX_ITEMS)
            .filter_map(|o| {
                let (lat, lon) = lat_lon(o)?;
                let slots_free = count(o, &["/slots_free", "/slots_available", "/available_slots"])
                    .or_else(|| {
                        let q = o.get("quota")?.as_u64()?;
                        let r = o.get("registered").and_then(Value::as_u64).unwrap_or(0);
                        Some(q.saturating_sub(r).min(1_000_000) as u32)
                    });
                let needed = o
                    .get("needed_blood_types")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .take(16)
                            .filter_map(|t| text(Some(t), 16))
                            .map(|t| t.to_lowercase())
                            .collect()
                    })
                    .unwrap_or_default();
                Some(DonationEvent {
                    id: text(o.get("id"), 64)?,
                    name: text(first(o, &["/name", "/title", "/location/name"]), 120)?,
                    organizer: text(o.get("organizer"), 120),
                    facility_id: text(o.get("facility_id"), 64),
                    address: text(o.get("address"), 200),
                    lat,
                    lon,
                    opens_at: time(o, &["/opens_at", "/starts_at", "/start_time"]),
                    closes_at: time(o, &["/closes_at", "/ends_at", "/end_time"]),
                    slots_total: count(o, &["/slots_total", "/quota"]),
                    slots_free,
                    capacity_per_hour: count(o, &["/capacity_per_hour"]),
                    status: text(o.get("status"), 20).map(|s| s.to_lowercase()),
                    needed_blood_types: needed,
                    updated_at: time(o, &["/updated_at"]),
                })
            })
            .collect())
    }

    type Site = (
        &'static str,
        &'static str,
        Option<u32>,
        &'static str,
        f64,
        f64,
    );
    const SITES: [Site; 6] = [
        (
            "Donor Darah UDD PMI Kota Bandung",
            "UDD PMI Kota Bandung",
            Some(1),
            "Jl. Aceh No. 79, Bandung",
            -6.9097,
            107.6175,
        ),
        (
            "Donor Darah RSUP Dr. Hasan Sadikin",
            "RSUP Dr. Hasan Sadikin",
            Some(2),
            "Jl. Pasteur No. 38, Bandung",
            -6.8945,
            107.5985,
        ),
        (
            "Donor Darah Kampus Ganesha",
            "Unit Donor Kampus",
            Some(1),
            "Jl. Ganesha No. 10, Bandung",
            -6.8915,
            107.6107,
        ),
        (
            "Mobil Unit Alun-Alun Bandung",
            "UDD PMI Kota Bandung",
            Some(1),
            "Alun-Alun Bandung",
            -6.9218,
            107.6071,
        ),
        (
            "Donor Darah RS Al Islam",
            "RS Al Islam Bandung",
            Some(4),
            "Jl. Soekarno-Hatta No. 644, Bandung",
            -6.9378,
            107.6667,
        ),
        (
            "Donor Darah Komunitas Cimahi",
            "Komunitas Donor Cimahi",
            None,
            "Alun-Alun Cimahi",
            -6.8722,
            107.5425,
        ),
    ];
    const GROUPS: [&str; 5] = [
        "o_positive",
        "o_negative",
        "a_negative",
        "b_negative",
        "ab_negative",
    ];

    /// `GET /donation-events?limit&offset` in DonorKita's exact shape: scheduled events that
    /// have not closed yet (an event that closed today moves to tomorrow), ordered by
    /// `opens_at, id`. Free slots fall as people reserve and refill every 10 minutes.
    pub fn mock_events(now: DateTime<Utc>, limit: usize, offset: usize) -> Value {
        let base = now
            .date_naive()
            .and_hms_opt(1, 0, 0)
            .expect("01:00")
            .and_utc(); // 08:00 WIB
        let minute = now.timestamp().div_euclid(60) as u64;
        let mut rows: Vec<(DateTime<Utc>, usize, Value)> = SITES
            .iter()
            .enumerate()
            .map(|(i, (name, organizer, fac, address, lat, lon))| {
                let hours = if i % 2 == 0 { 5 } else { 4 };
                let cap = 8 + i as u32;
                let mut opens = base + Duration::minutes(30 * i as i64);
                while opens + Duration::hours(hours) < now {
                    opens += Duration::days(1);
                }
                let closes = opens + Duration::hours(hours);
                let total = hours as u64 * cap as u64;
                let taken = ((minute % 10) * total / 10 + i as u64 * 3) % total;
                let needed: Vec<&str> = GROUPS
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| (minute / 5 + (i + k) as u64).is_multiple_of(3))
                    .map(|(_, g)| *g)
                    .collect();
                let updated = DateTime::from_timestamp((minute * 60) as i64, 0).unwrap_or(now);
                let v = json!({
                    "id": format!("evt_{}", i + 1),
                    "name": name,
                    "organizer": organizer,
                    "facility_id": fac.map(|f| format!("fac_{f}")),
                    "address": address,
                    "lat": lat,
                    "lon": lon,
                    "opens_at": iso(opens),
                    "closes_at": iso(closes),
                    "slots_total": total,
                    "slots_free": total - taken,
                    "capacity_per_hour": cap,
                    "status": "scheduled",
                    "needed_blood_types": needed,
                    "updated_at": iso(updated),
                });
                (opens, i, v)
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let rows: Vec<Value> = rows.into_iter().map(|r| r.2).collect();
        json!({"data": page_of(&rows, limit, offset), "meta": mock_meta(now, minute as i64, rows.len())})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    const TOURISM_KEYS: [&str; 6] = ["id", "name", "category", "lat", "lon", "updated_at"];
    const HEALTH_KEYS: [&str; 15] = [
        "id",
        "name",
        "organizer",
        "facility_id",
        "address",
        "lat",
        "lon",
        "opens_at",
        "closes_at",
        "slots_total",
        "slots_free",
        "capacity_per_hour",
        "status",
        "needed_blood_types",
        "updated_at",
    ];
    const META_KEYS: [&str; 5] = ["seq", "updated_at", "source", "stale", "total"];

    fn keys(v: &Value) -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    }

    fn sorted(k: &[&str]) -> Vec<String> {
        let mut k: Vec<String> = k.iter().map(|s| s.to_string()).collect();
        k.sort();
        k
    }

    /// The mock must have exactly the keys of the real sample (no more, no fewer).
    #[test]
    fn mocks_have_the_real_keys() {
        let now = t("2026-11-28T03:00:00Z");
        let real_t: Value = serde_json::from_str(tourism::SAMPLE).unwrap();
        let mock_t = tourism::mock_feed(now, 50, 0);
        assert_eq!(keys(&real_t["data"][0]), sorted(&TOURISM_KEYS));
        assert_eq!(keys(&mock_t["data"][0]), sorted(&TOURISM_KEYS));
        assert_eq!(keys(&mock_t["meta"]), sorted(&META_KEYS));
        let real_h: Value = serde_json::from_str(health::SAMPLE).unwrap();
        let mock_h = health::mock_events(now, 50, 0);
        assert_eq!(keys(&real_h["data"][0]), sorted(&HEALTH_KEYS));
        assert_eq!(keys(&mock_h["data"][0]), sorted(&HEALTH_KEYS));
        assert_eq!(keys(&mock_h["meta"]), sorted(&META_KEYS));
        assert_eq!(keys(&real_t["meta"]), sorted(&META_KEYS));
    }

    #[test]
    fn real_tourism_sample_parses() {
        let v: Value = serde_json::from_str(tourism::SAMPLE).unwrap();
        let d = tourism::parse(&v).unwrap();
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].id, "1");
        assert_eq!(d[0].name, "Kampung Adat Cireundeu");
        assert_eq!(d[0].category, "culture_history");
        assert_eq!(d[0].updated_at, Some(t("2026-11-27T02:15:00Z")));
    }

    #[test]
    fn real_health_sample_parses_slots_free() {
        let v: Value = serde_json::from_str(health::SAMPLE).unwrap();
        let e = health::parse(&v).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].id, "evt_1");
        assert_eq!(e[0].slots_free, Some(47), "DonorKita sends slots_free");
        assert_eq!(e[0].slots_total, Some(50));
        assert_eq!(e[0].facility_id.as_deref(), Some("fac_1"));
        assert_eq!(e[0].opens_at, Some(t("2026-11-29T01:00:00Z")));
        assert_eq!(e[0].needed_blood_types, vec!["ab_negative", "o_negative"]);
        assert_eq!(e[1].slots_free, Some(32));
    }

    #[test]
    fn mock_tourism_uses_real_ids_and_categories() {
        let now = t("2026-11-28T03:00:00Z");
        let v = tourism::mock_feed(now, 50, 0);
        let d = tourism::parse(&v).unwrap();
        assert_eq!(d.len(), 11);
        assert_eq!(v["meta"]["total"], 11);
        assert!(d.iter().all(|x| x.id.parse::<u32>().is_ok()));
        assert!(
            d.iter()
                .all(|x| tourism::CATEGORIES.contains(&x.category.as_str()))
        );
        let later =
            tourism::parse(&tourism::mock_feed(now + Duration::seconds(30), 50, 0)).unwrap();
        assert_ne!(d, later, "a different destination is hidden every 30 s");
    }

    #[test]
    fn mock_health_is_scheduled_open_and_changing() {
        let now = t("2026-11-28T03:00:00Z");
        let e = health::parse(&health::mock_events(now, 50, 0)).unwrap();
        assert_eq!(e.len(), 6);
        assert!(e.iter().all(|x| x.closes_at.unwrap() >= now));
        assert!(e.iter().all(|x| x.status.as_deref() == Some("scheduled")));
        assert!(e.windows(2).all(|w| w[0].opens_at <= w[1].opens_at));
        assert!(
            e.iter()
                .all(|x| x.slots_free.unwrap() <= x.slots_total.unwrap())
        );
        let later = health::parse(&health::mock_events(now + Duration::minutes(3), 50, 0)).unwrap();
        let by_id = |v: &[health::DonationEvent], id: &str| {
            v.iter().find(|x| x.id == id).unwrap().slots_free
        };
        assert_ne!(by_id(&e, "evt_1"), by_id(&later, "evt_1"));
        // Late in the day every event has moved to tomorrow, so the list is never empty.
        let night = t("2026-11-28T15:00:00Z");
        assert_eq!(
            health::parse(&health::mock_events(night, 50, 0))
                .unwrap()
                .len(),
            6
        );
    }

    #[test]
    fn mocks_honour_limit_and_offset() {
        let now = t("2026-11-28T03:00:00Z");
        let v = tourism::mock_feed(now, 5, 10);
        assert_eq!(v["data"].as_array().unwrap().len(), 1);
        assert_eq!(v["meta"]["total"], 11);
        let v = health::mock_events(now, 2, 0);
        assert_eq!(v["data"].as_array().unwrap().len(), 2);
        assert_eq!(v["meta"]["total"], 6);
    }

    #[test]
    fn paginator_follows_meta_total() {
        let page = |n: usize, total: usize| json!({"data": vec![json!({"id": "x"}); n], "meta": {"total": total, "seq": 1}});
        let mut p = Paginator::new();
        assert_eq!(p.next_offset(), Some(0));
        p.push(page(200, 450)).unwrap();
        assert_eq!(p.next_offset(), Some(200));
        p.push(page(200, 450)).unwrap();
        assert_eq!(p.next_offset(), Some(400));
        p.push(page(50, 450)).unwrap();
        assert_eq!(p.next_offset(), None);
        let all = p.finish();
        assert_eq!(all["data"].as_array().unwrap().len(), 450);
        assert_eq!(all["meta"]["total"], 450);

        let mut p = Paginator::new();
        for _ in 0..MAX_PAGES {
            p.push(page(200, 100_000)).unwrap();
        }
        assert_eq!(p.next_offset(), None, "stops after MAX_PAGES");
        assert!(Paginator::new().push(json!({"error": "x"})).is_err());
    }

    #[test]
    fn untrusted_fields_are_cleaned() {
        let v = json!({"data": [
            {"id": 7, "name": "Curug\u{0007} Dago", "category": "Nature", "lat": "-6.87", "lon": 107.62, "extra": "ignored"},
            {"id": "y", "lat": -6.9, "lon": 107.6},
            {"id": "z", "name": "Bad", "lat": 91.0, "lon": 107.6},
        ]});
        let got = tourism::parse(&v).unwrap();
        assert_eq!(
            got.len(),
            1,
            "nameless and out-of-range entries are dropped"
        );
        assert_eq!(got[0].name, "Curug Dago");
        assert_eq!(got[0].id, "7");
        assert_eq!(got[0].category, "nature");
        assert!(tourism::parse(&json!({"error": "nope"})).is_err());
        let h = health::parse(&json!({"data": [
            {"id": "evt_9", "name": "Old shape", "location": {"lat": -6.9, "lng": 107.6}, "quota": 10, "registered": 4}
        ]}))
        .unwrap();
        assert_eq!(h[0].slots_free, Some(6));
    }
}
