//! Mock generator (AGENTS.md section 7). It randomizes inputs only: weather, reports, votes,
//! disaster signals and upstream faults. Verification, risk and the change feed run the real code.

mod rng;

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::config::MockConfig;
use crate::engine::World;
use crate::reports::{Category, NewReport, ReportStatus};
use crate::risk::inputs::{DisasterKind, Storm, WeatherState};
use crate::rules::{MAX_LIVE_REPORTS_MOCK, MOCK_AREA};
use crate::verification::Vote;
use rng::Rng;

pub const SCENARIOS: [&str; 8] = [
    "rain_storm",
    "flood_verified",
    "road_closure",
    "event_crowd",
    "disaster_signal",
    "upstream_down",
    "recover",
    "reset",
];

const CATEGORIES: [(Category, f64); 5] = [
    (Category::Flood, 0.30),
    (Category::Accident, 0.25),
    (Category::RoadDamage, 0.15),
    (Category::RoadClosure, 0.15),
    (Category::EventCrowd, 0.15),
];
pub const UPSTREAMS: [&str; 5] = ["open_meteo", "petabencana", "osrm", "tourism", "health"];
const USERS: usize = 20;

fn description(c: Category, n: i64) -> String {
    let opts: &[&str] = match c {
        Category::Flood => &[
            "Genangan setinggi mata kaki",
            "Air naik sampai 30 cm",
            "Banjir di depan pertokoan",
        ],
        Category::Accident => &[
            "Tabrakan dua motor",
            "Mobil tergelincir",
            "Kecelakaan di persimpangan",
        ],
        Category::RoadDamage => &[
            "Lubang besar di lajur kiri",
            "Aspal retak dan amblas",
            "Jalan bergelombang",
        ],
        Category::RoadClosure => &[
            "Jalan ditutup untuk perbaikan",
            "Penutupan karena pohon tumbang",
            "Akses ditutup petugas",
        ],
        Category::EventCrowd => &["Konser di lapangan", "Pawai warga", "Pasar malam padat"],
    };
    opts[n as usize % opts.len()].to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct MockState {
    pub seed: u64,
    pub tick_seconds: u64,
    pub time_scale: u32,
    pub tick: u64,
    pub active_scenario: Option<String>,
    pub live_reports: usize,
    pub active_disasters: usize,
    pub upstreams_down: Vec<String>,
}

pub struct Generator {
    rng: Rng,
    seed: u64,
    tick_seconds: u64,
    time_scale: u32,
    tick: u64,
    users: Vec<String>,
    /// Hidden truth per report: true reports mostly get confirmed, false ones disputed.
    truth: HashMap<String, bool>,
    disaster_ends: HashMap<String, DateTime<Utc>>,
    idle_ticks: u32,
    last_seq: u64,
    active_scenario: Option<String>,
}

fn random_point(rng: &mut Rng) -> (f64, f64) {
    (
        rng.range(MOCK_AREA.min_lat, MOCK_AREA.max_lat),
        rng.range(MOCK_AREA.min_lon, MOCK_AREA.max_lon),
    )
}

impl Generator {
    pub fn new(cfg: &MockConfig, world: &mut World, now: DateTime<Utc>) -> Self {
        let mut g = Self {
            rng: Rng::new(cfg.seed),
            seed: cfg.seed,
            tick_seconds: cfg.tick_seconds,
            time_scale: cfg.time_scale,
            tick: 0,
            users: (1..=USERS).map(|i| format!("user_{i:02}")).collect(),
            truth: HashMap::new(),
            disaster_ends: HashMap::new(),
            idle_ticks: 0,
            last_seq: 0,
            active_scenario: None,
        };
        g.seed_world(world, now);
        g
    }

    fn seed_world(&mut self, w: &mut World, now: DateTime<Utc>) {
        for u in &self.users {
            let rep = self.rng.range(0.2, 1.0);
            w.verification.set_reputation(u, rep);
        }
        let weather = WeatherState {
            available: true,
            observed_at: now,
            base_rain_mm_h: self.rng.range(0.0, 3.0),
            storm: None,
            temperature_offset_c: 0.0,
            live_forecast: None,
        };
        w.set_weather(weather, now);
    }

    pub fn state(&self, w: &World, now: DateTime<Utc>) -> MockState {
        MockState {
            seed: self.seed,
            tick_seconds: self.tick_seconds,
            time_scale: self.time_scale,
            tick: self.tick,
            active_scenario: self.active_scenario.clone(),
            live_reports: w.live_reports().count(),
            active_disasters: w.active_disasters().len(),
            upstreams_down: w.stale_upstreams(now),
        }
    }

    pub fn tick(&mut self, w: &mut World, now: DateTime<Utc>) {
        self.tick += 1;

        if w.stale_upstream(now).is_none() && self.rng.chance(0.03) {
            let up = UPSTREAMS[self.rng.int(0, UPSTREAMS.len() as i64 - 1) as usize].to_string();
            w.set_stale(Some((up, now + Duration::seconds(self.rng.int(20, 40)))));
        }

        self.weather_step(w, now, self.idle_ticks >= 3);
        self.resolve_due_disasters(w, now);

        for _ in 0..self.rng.int(1, 3) {
            match self.rng.weighted(&[0.45, 0.40, 0.15]) {
                0 => self.new_report(w, now),
                1 => self.vote_round(w, now),
                _ => self.maybe_disaster(w, now),
            }
        }

        if now - w.last_transition_at() > Duration::seconds(120) {
            self.rain_storm(w, now, None);
            self.active_scenario = Some("rain_storm".into());
        }

        if w.seq() == self.last_seq {
            self.idle_ticks += 1;
        } else {
            self.idle_ticks = 0;
        }
        self.last_seq = w.seq();
        let live: std::collections::HashSet<&str> =
            w.live_reports().map(|r| r.id.as_str()).collect();
        self.truth.retain(|id, _| live.contains(id.as_str()));
    }

    fn weather_step(&mut self, w: &mut World, now: DateTime<Utc>, force: bool) {
        let mut next = w.weather().clone();
        let mut step = self.rng.range(-4.0, 4.0);
        if force && step.abs() < 0.5 {
            step = if step < 0.0 { -1.0 } else { 1.0 };
        }
        next.base_rain_mm_h =
            (next.base_rain_mm_h + step - next.base_rain_mm_h * 0.15).clamp(0.0, 60.0);
        if next.storm.as_ref().is_some_and(|s| s.ends_at <= now) {
            next.storm = None;
        }
        let dry = next.base_rain_mm_h < 1.0 && next.storm.is_none();
        if dry && self.rng.chance(0.10) {
            let peak = self.rng.range(25.0, 45.0);
            next.storm = Some(self.make_storm(None, now, peak));
        }
        next.temperature_offset_c = self.rng.range(-0.3, 0.3);
        next.observed_at = now;
        next.available = true;
        w.set_weather(next, now);
    }

    fn make_storm(&mut self, at: Option<(f64, f64)>, now: DateTime<Utc>, peak: f64) -> Storm {
        let (lat, lon) = at.unwrap_or_else(|| random_point(&mut self.rng));
        Storm {
            lat,
            lon,
            radius_m: self.rng.range(1500.0, 3000.0),
            peak_mm_h: peak,
            ends_at: now + Duration::seconds(self.rng.int(60, 120)),
        }
    }

    fn new_report(&mut self, w: &mut World, now: DateTime<Utc>) {
        if w.live_reports().count() >= MAX_LIVE_REPORTS_MOCK {
            return;
        }
        let category = CATEGORIES[self.rng.weighted(&CATEGORIES.map(|c| c.1))].0;
        let (lat, lon) = random_point(&mut self.rng);
        let reporter = self.users[self.rng.int(0, USERS as i64 - 1) as usize].clone();
        let new = NewReport {
            category,
            lat,
            lon,
            description: Some(description(category, self.rng.int(0, 2))),
            severity: self.rng.int(1, 3) as u8,
            reporter,
            origin: Default::default(),
        };
        if let Ok(r) = w.create_report(new, now) {
            let real = self.rng.chance(0.8);
            self.truth.insert(r.id, real);
        }
    }

    fn vote_round(&mut self, w: &mut World, now: DateTime<Utc>) {
        let waiting: Vec<String> = w
            .live_reports()
            .filter(|r| r.status == ReportStatus::Waiting)
            .map(|r| r.id.clone())
            .collect();
        if waiting.is_empty() {
            return;
        }
        for _ in 0..self.rng.int(1, 3) {
            let id = &waiting[self.rng.int(0, waiting.len() as i64 - 1) as usize];
            let real = self.truth.get(id).copied().unwrap_or(true);
            for _ in 0..self.rng.int(1, 2) {
                let user = self.users[self.rng.int(0, USERS as i64 - 1) as usize].clone();
                let confirm = self.rng.chance(if real { 0.85 } else { 0.25 });
                let vote = if confirm {
                    Vote::Confirm
                } else {
                    Vote::Dispute
                };
                let _ = w.vote(id, &user, vote, now); // own/double votes are simply skipped
            }
        }
    }

    fn maybe_disaster(&mut self, w: &mut World, now: DateTime<Utc>) {
        if w.active_disasters().len() >= 3 || !self.rng.chance(0.5) {
            return;
        }
        let kind = match self.rng.weighted(&[0.7, 0.15, 0.15]) {
            0 => DisasterKind::Flood,
            1 => DisasterKind::Wind,
            _ => DisasterKind::Fire,
        };
        let p = random_point(&mut self.rng);
        self.spawn_disaster(w, kind, p, now);
    }

    fn spawn_disaster(
        &mut self,
        w: &mut World,
        kind: DisasterKind,
        (lat, lon): (f64, f64),
        now: DateTime<Utc>,
    ) {
        let id = w.spawn_disaster(kind, lat, lon, now);
        self.disaster_ends
            .insert(id, now + Duration::seconds(self.rng.int(120, 300)));
    }

    fn resolve_due_disasters(&mut self, w: &mut World, now: DateTime<Utc>) {
        let due: Vec<String> = self
            .disaster_ends
            .iter()
            .filter(|(_, t)| **t <= now)
            .map(|(id, _)| id.clone())
            .collect();
        for id in due {
            self.disaster_ends.remove(&id);
            w.resolve_disaster(&id, now);
        }
    }

    fn rain_storm(&mut self, w: &mut World, now: DateTime<Utc>, at: Option<(f64, f64)>) {
        let mut next = w.weather().clone();
        let peak = self.rng.range(40.0, 45.0);
        next.storm = Some(self.make_storm(at, now, peak));
        next.observed_at = now;
        next.available = true;
        w.set_weather(next, now);
    }

    /// Creates a severe report and votes it to `verified` with the most trusted users.
    fn verified_report(
        &mut self,
        w: &mut World,
        category: Category,
        at: Option<(f64, f64)>,
        now: DateTime<Utc>,
    ) {
        let (lat, lon) = at.unwrap_or_else(|| random_point(&mut self.rng));
        let new = NewReport {
            category,
            lat,
            lon,
            description: Some(description(category, 0)),
            severity: 3,
            reporter: "user_01".into(),
            origin: Default::default(),
        };
        let Ok(r) = w.create_report(new, now) else {
            return;
        };
        self.truth.insert(r.id.clone(), true);
        let mut voters: Vec<String> = self.users[1..].to_vec();
        voters.sort_by(|a, b| {
            w.verification
                .reputation(b)
                .total_cmp(&w.verification.reputation(a))
        });
        for u in voters {
            match w.vote(&r.id, &u, Vote::Confirm, now) {
                Ok(rep) if rep.status == ReportStatus::Verified => break,
                _ => {}
            }
        }
    }

    pub fn scenario(
        &mut self,
        w: &mut World,
        name: &str,
        at: Option<(f64, f64)>,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        self.scenario_on(w, name, at, None, now)
    }

    pub fn scenario_on(
        &mut self,
        w: &mut World,
        name: &str,
        at: Option<(f64, f64)>,
        upstream: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        let upstream = upstream.unwrap_or("open_meteo");
        if !UPSTREAMS.contains(&upstream) {
            return Err(format!(
                "upstream harus salah satu dari: {}",
                UPSTREAMS.join(", ")
            ));
        }
        match name {
            "rain_storm" => self.rain_storm(w, now, at),
            "flood_verified" => self.verified_report(w, Category::Flood, at, now),
            "road_closure" => self.verified_report(w, Category::RoadClosure, at, now),
            "event_crowd" => self.verified_report(w, Category::EventCrowd, at, now),
            "disaster_signal" => {
                let p = at.unwrap_or_else(|| random_point(&mut self.rng));
                self.spawn_disaster(w, DisasterKind::Flood, p, now);
            }
            "upstream_down" => w.set_stale(Some((upstream.into(), now + Duration::seconds(30)))),
            "recover" => {
                w.set_stale(None);
                let mut next = w.weather().clone();
                next.storm = None;
                next.base_rain_mm_h = 0.0;
                next.observed_at = now;
                w.set_weather(next, now);
            }
            "reset" => {
                w.reset_inputs(now);
                self.truth.clear();
                self.disaster_ends.clear();
                self.seed_world(w, now);
            }
            other => return Err(format!("Skenario tidak dikenal: {other}")),
        }
        self.active_scenario = (name != "reset" && name != "recover").then(|| name.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::engine::Engine;
    use crate::risk::RiskLevel;
    use crate::risk::grid::CellId;

    fn engine(seed: u64) -> (Engine, DateTime<Utc>) {
        let cfg = Config::from_lookup(|k| match k {
            "MOCK_MODE" => Some("true".into()),
            "MOCK_SEED" => Some(seed.to_string()),
            _ => None,
        })
        .unwrap();
        let t0 = "2026-11-22T00:00:00Z".parse().unwrap();
        (Engine::new(&cfg, t0), t0)
    }

    fn run(seed: u64, ticks: i64) -> Vec<String> {
        let (e, t0) = engine(seed);
        for i in 1..=ticks {
            e.tick(t0 + Duration::seconds(5 * i));
        }
        e.snapshot()
            .changes
            .iter()
            .map(|c| format!("{} {:?} {}", c.seq, c.kind, c.entity_id))
            .collect()
    }

    #[test]
    fn same_seed_same_story() {
        assert_eq!(run(42, 60), run(42, 60));
        assert_ne!(run(42, 60), run(43, 60));
    }

    #[test]
    fn seq_advances_every_tick() {
        let (e, t0) = engine(7);
        let mut last = e.snapshot().seq;
        for i in 1..=100 {
            e.tick(t0 + Duration::seconds(5 * i));
            let s = e.snapshot().seq;
            assert!(s > last, "seq did not advance at tick {i}");
            last = s;
        }
    }

    #[test]
    fn ten_minutes_produce_every_kind_of_change() {
        let (e, t0) = engine(11);
        for i in 1..=120 {
            e.tick(t0 + Duration::seconds(5 * i));
        }
        let s = e.snapshot();
        for kind in [
            "ReportCreated",
            "ReportVerified",
            "ReportExpired",
            "RiskChanged",
            "WeatherUpdated",
            "DisasterCreated",
        ] {
            assert!(
                s.changes.iter().any(|c| format!("{:?}", c.kind) == kind),
                "no {kind} in 10 minutes"
            );
        }
        assert!(
            s.reports
                .iter()
                .filter(|r| r.status != ReportStatus::Expired)
                .count()
                <= MAX_LIVE_REPORTS_MOCK
        );
    }

    #[test]
    fn invariants_hold_throughout() {
        let (e, t0) = engine(99);
        for i in 1..=200 {
            let now = t0 + Duration::seconds(5 * i);
            e.tick(now);
            let s = e.snapshot();
            for r in &s.reports {
                if r.status == ReportStatus::Verified {
                    assert!(r.expires_at > now, "verified report past expiry still live");
                }
            }
            let calm = WeatherState {
                available: false,
                ..s.weather.clone()
            };
            let from_verified_only = crate::risk::RiskGrid::compute(
                &s.reports
                    .iter()
                    .filter(|r| r.status == ReportStatus::Verified)
                    .cloned()
                    .collect::<Vec<_>>(),
                &[],
                &calm,
                now,
            );
            let all = crate::risk::RiskGrid::compute(&s.reports, &[], &calm, now);
            let c = CellId::of(-6.9, 107.6).unwrap();
            assert_eq!(
                from_verified_only.get(c).score,
                all.get(c).score,
                "waiting reports raised risk"
            );
        }
    }

    #[test]
    fn upstream_down_can_target_a_partner() {
        let (e, t0) = engine(3);
        e.update_mock(t0, |g, w| {
            g.scenario_on(w, "upstream_down", None, Some("health"), t0)
        })
        .unwrap()
        .unwrap();
        let s = e.snapshot();
        assert!(s.stale);
        assert_eq!(
            s.mock.as_ref().unwrap().upstreams_down,
            vec!["health".to_string()]
        );
        let bad = e
            .update_mock(t0, |g, w| {
                g.scenario_on(w, "upstream_down", None, Some("nope"), t0)
            })
            .unwrap();
        assert!(bad.is_err());
    }

    #[test]
    fn flood_verified_scenario_raises_risk_at_point() {
        let (e, t0) = engine(5);
        let p = (-6.9, 107.6);
        for _ in 0..2 {
            e.update_mock(t0, |g, w| g.scenario(w, "flood_verified", Some(p), t0))
                .unwrap()
                .unwrap();
        }
        let s = e.snapshot();
        assert_eq!(
            s.risk.get(CellId::of(p.0, p.1).unwrap()).level,
            RiskLevel::Danger
        );
        assert!(
            s.changes
                .iter()
                .any(|c| c.kind == crate::feed::ChangeType::RiskChanged)
        );
    }
}

#[cfg(test)]
mod perf {
    /// `cargo test --release -p core-service tick_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn tick_cost() {
        let cfg = crate::config::Config::from_lookup(|k| match k {
            "MOCK_MODE" => Some("true".into()),
            "MOCK_SEED" => Some("3".into()),
            _ => None,
        })
        .unwrap();
        let t0: chrono::DateTime<chrono::Utc> = "2026-11-22T00:00:00Z".parse().unwrap();
        let e = crate::engine::Engine::new(&cfg, t0);
        let n = 500;
        let start = std::time::Instant::now();
        for i in 1..=n {
            e.tick(t0 + chrono::Duration::seconds(5 * i));
        }
        let per = start.elapsed() / n as u32;
        let s = e.snapshot();
        println!(
            "tick: {per:?} avg over {n} ticks; live reports {}; changes kept {}",
            s.reports.len(),
            s.changes.len()
        );
    }
}
