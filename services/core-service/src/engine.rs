//! Wires the bounded contexts together through domain events, and publishes an immutable
//! snapshot for readers. One writer (the `World` mutex); readers only clone an `Arc`.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use chrono::{DateTime, Duration, Utc};

use crate::alerts::{Subscription, Watch};
use crate::config::Config;
use crate::feed::{Change, ChangeFeed, ChangeType};
use crate::http::envelope::Source;
use crate::mock::{Generator, MockState};
use crate::reports::{Category, NewReport, Origin, Report, ReportStatus};
use crate::risk::RiskGrid;
use crate::risk::grid::CellId;
use crate::risk::inputs::{DisasterKind, DisasterSignal, DisasterStatus, WeatherState};
use crate::rules::*;
use crate::store::{Loaded, PersistOp};
use crate::verification::{Verification, Vote, VoteError, VoteOutcome};

/// Domain events, named as in the DDD paper. Each becomes one change-feed entry.
#[derive(Debug, Clone)]
pub enum DomainEvent {
    LaporanDibuat {
        id: String,
        category: Category,
    },
    LaporanTerverifikasi {
        id: String,
        category: Category,
    },
    LaporanKedaluwarsa {
        id: String,
        category: Category,
        disputed: bool,
    },
    TingkatRisikoBerubah {
        cell: CellId,
        from: crate::risk::RiskLevel,
        to: crate::risk::RiskLevel,
    },
    CuacaDiperbarui {
        rain_mm_h: f64,
    },
    SinyalBencanaMuncul {
        id: String,
        kind: DisasterKind,
    },
    SinyalBencanaSelesai {
        id: String,
        kind: DisasterKind,
    },
}

pub struct World {
    source: Source,
    mock_expiry_scale: Option<u32>,
    reports: Vec<Report>,
    archive: VecDeque<Report>,
    pub verification: Verification,
    disasters: Vec<DisasterSignal>,
    resolved: VecDeque<DisasterSignal>,
    weather: WeatherState,
    risk: RiskGrid,
    feed: ChangeFeed,
    next_report: u64,
    next_disaster: u64,
    /// Upstreams currently down -> until when to report `meta.stale`.
    stale: BTreeMap<String, DateTime<Utc>>,
    /// Subscribed areas, checked after every risk recompute.
    watches: Vec<Watch>,
    /// Changes to store, drained by the engine after each write.
    outbox: Vec<PersistOp>,
    last_transition_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl World {
    pub fn new(source: Source, mock_expiry_scale: Option<u32>, now: DateTime<Utc>) -> Self {
        Self {
            source,
            mock_expiry_scale,
            reports: Vec::new(),
            archive: VecDeque::new(),
            verification: Verification::default(),
            disasters: Vec::new(),
            resolved: VecDeque::new(),
            weather: WeatherState::unavailable(now),
            risk: RiskGrid::empty(),
            feed: ChangeFeed::default(),
            next_report: 0,
            next_disaster: 0,
            stale: BTreeMap::new(),
            watches: Vec::new(),
            outbox: Vec::new(),
            last_transition_at: now,
            updated_at: now,
        }
    }

    fn emit(&mut self, ev: DomainEvent, now: DateTime<Utc>) {
        let (kind, entity, summary) = match ev {
            DomainEvent::LaporanDibuat { id, category } => (
                ChangeType::ReportCreated,
                id,
                format!("Laporan baru: {}", category.label_id()),
            ),
            DomainEvent::LaporanTerverifikasi { id, category } => (
                ChangeType::ReportVerified,
                id,
                format!("{} terverifikasi", category.label_id()),
            ),
            DomainEvent::LaporanKedaluwarsa {
                id,
                category,
                disputed,
            } => {
                let s = if disputed {
                    "dibantah warga"
                } else {
                    "kedaluwarsa"
                };
                (
                    ChangeType::ReportExpired,
                    id,
                    format!("{} {s}", category.label_id()),
                )
            }
            DomainEvent::TingkatRisikoBerubah { cell, from, to } => {
                self.last_transition_at = now;
                (
                    ChangeType::RiskChanged,
                    cell.to_string(),
                    format!("{} → {}", from.label_id(), to.label_id()),
                )
            }
            DomainEvent::CuacaDiperbarui { rain_mm_h } => (
                ChangeType::WeatherUpdated,
                "bandung".into(),
                format!("Hujan {rain_mm_h:.1} mm/jam"),
            ),
            DomainEvent::SinyalBencanaMuncul { id, kind } => (
                ChangeType::DisasterCreated,
                id,
                format!("Sinyal {} dari PetaBencana", kind.label_id()),
            ),
            DomainEvent::SinyalBencanaSelesai { id, kind } => (
                ChangeType::DisasterResolved,
                id,
                format!("Sinyal {} selesai", kind.label_id()),
            ),
        };
        self.feed.push(now, kind, entity, summary);
        self.updated_at = now;
    }

    pub fn seq(&self) -> u64 {
        self.feed.last_seq()
    }

    pub fn last_transition_at(&self) -> DateTime<Utc> {
        self.last_transition_at
    }

    pub fn expiry_s(&self, c: Category) -> i64 {
        match self.mock_expiry_scale {
            Some(scale) => (default_expiry_s(c) / scale as i64).min(MOCK_EXPIRY_CAP_S),
            None => default_expiry_s(c),
        }
    }

    // ---- Pelaporan Insiden ----

    pub fn create_report(&mut self, new: NewReport, now: DateTime<Utc>) -> Result<Report, String> {
        let cell = new.validate()?;
        self.next_report += 1;
        let r = Report {
            id: format!("rpt_{}", self.next_report),
            category: new.category,
            status: ReportStatus::Waiting,
            lat: new.lat,
            lon: new.lon,
            description: new.description,
            severity: new.severity,
            confirm_count: 0,
            dispute_count: 0,
            created_at: now,
            verified_at: None,
            expires_at: now + Duration::seconds(self.expiry_s(new.category)),
            reporter: new.reporter,
            cell,
            origin: new.origin,
        };
        self.reports.push(r.clone());
        self.persist_report(&r);
        self.emit(
            DomainEvent::LaporanDibuat {
                id: r.id.clone(),
                category: r.category,
            },
            now,
        );
        Ok(r)
    }

    pub fn live_reports(&self) -> impl Iterator<Item = &Report> {
        self.reports.iter()
    }

    fn persist_report(&mut self, r: &Report) {
        if r.origin == Origin::User {
            self.outbox.push(PersistOp::UpsertReport(r.clone()));
        }
    }

    fn persist_reputation(&mut self, user: &str) {
        // Mock users (`user_01`..) are reseeded on start; only app users (`usr_...`) are stored.
        if user.starts_with("usr_") {
            self.outbox.push(PersistOp::Reputation {
                user_id: user.to_string(),
                value: self.verification.reputation(user),
            });
        }
    }

    /// Reports `user` created in the last hour (live or archived), for the per-user rate limit.
    pub fn recent_reports_by(&self, user: &str, now: DateTime<Utc>) -> usize {
        let since = now - Duration::hours(1);
        self.reports
            .iter()
            .chain(self.archive.iter())
            .filter(|r| r.reporter == user && r.created_at >= since)
            .count()
    }

    // ---- Verifikasi & Reputasi ----

    pub fn vote(
        &mut self,
        report_id: &str,
        user: &str,
        vote: Vote,
        now: DateTime<Utc>,
    ) -> Result<Report, VoteError> {
        let i = self
            .reports
            .iter()
            .position(|r| r.id == report_id)
            .ok_or(VoteError::NotFound)?;
        let weight = self.verification.reputation(user);
        let outcome = self
            .verification
            .vote(&mut self.reports[i], user, vote, now)?;
        let r = self.reports[i].clone();
        if r.origin == Origin::User {
            self.outbox.push(PersistOp::Vote {
                report_id: r.id.clone(),
                user_id: user.to_string(),
                vote,
                weight,
                at: now,
            });
            self.persist_report(&r);
        }
        if outcome != VoteOutcome::Counted {
            self.persist_reputation(&r.reporter);
        }
        match outcome {
            VoteOutcome::Counted => {}
            VoteOutcome::Verified => self.emit(
                DomainEvent::LaporanTerverifikasi {
                    id: r.id.clone(),
                    category: r.category,
                },
                now,
            ),
            VoteOutcome::Disputed => {
                self.reports.remove(i);
                self.archive_report(r.clone());
                self.emit(
                    DomainEvent::LaporanKedaluwarsa {
                        id: r.id.clone(),
                        category: r.category,
                        disputed: true,
                    },
                    now,
                );
            }
        }
        Ok(r)
    }

    fn archive_report(&mut self, r: Report) {
        if self.archive.len() == ARCHIVE_CAPACITY {
            self.archive.pop_back();
        }
        self.archive.push_front(r);
    }

    fn expire_due(&mut self, now: DateTime<Utc>) {
        let (due, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.reports)
            .into_iter()
            .partition(|r| r.expires_at <= now);
        self.reports = keep;
        for mut r in due {
            r.status = ReportStatus::Expired;
            self.verification.forget(&r.id);
            self.persist_report(&r);
            let ev = DomainEvent::LaporanKedaluwarsa {
                id: r.id.clone(),
                category: r.category,
                disputed: false,
            };
            self.archive_report(r);
            self.emit(ev, now);
        }
    }

    // ---- Inputs from adapters / mock ----

    pub fn weather(&self) -> &WeatherState {
        &self.weather
    }

    pub fn set_weather(&mut self, w: WeatherState, now: DateTime<Utc>) {
        let changed = !self.weather.available
            || w.observed_at != self.weather.observed_at
            || (w.base_rain_mm_h - self.weather.base_rain_mm_h).abs() >= 0.05
            || w.storm.is_some() != self.weather.storm.is_some()
            || (w.temperature_offset_c - self.weather.temperature_offset_c).abs() >= 0.05;
        let rain = w.rain_at(-6.9175, 107.6191); // Alun-alun Bandung as the city summary point
        self.weather = w;
        if changed {
            self.emit(DomainEvent::CuacaDiperbarui { rain_mm_h: rain }, now);
        }
    }

    pub fn active_disasters(&self) -> &[DisasterSignal] {
        &self.disasters
    }

    pub fn spawn_disaster(
        &mut self,
        kind: DisasterKind,
        lat: f64,
        lon: f64,
        now: DateTime<Utc>,
    ) -> String {
        self.next_disaster += 1;
        let id = format!("dis_{}", self.next_disaster);
        self.disasters.push(DisasterSignal {
            id: id.clone(),
            kind,
            lat,
            lon,
            reported_at: now,
            status: DisasterStatus::Active,
            resolved_at: None,
        });
        self.emit(
            DomainEvent::SinyalBencanaMuncul {
                id: id.clone(),
                kind,
            },
            now,
        );
        id
    }

    pub fn resolve_disaster(&mut self, id: &str, now: DateTime<Utc>) {
        if let Some(i) = self.disasters.iter().position(|d| d.id == id) {
            let mut d = self.disasters.remove(i);
            d.status = DisasterStatus::Resolved;
            d.resolved_at = Some(now);
            let ev = DomainEvent::SinyalBencanaSelesai {
                id: d.id.clone(),
                kind: d.kind,
            };
            if self.resolved.len() == RESOLVED_DISASTERS_CAPACITY {
                self.resolved.pop_back();
            }
            self.resolved.push_front(d);
            self.emit(ev, now);
        }
    }

    /// Mock faults: `Some` marks one upstream down until a time, `None` clears every fault.
    pub fn set_stale(&mut self, upstream: Option<(String, DateTime<Utc>)>) {
        match upstream {
            Some((u, until)) => {
                self.stale.insert(u, until);
            }
            None => self.stale.clear(),
        }
    }

    /// Adapters: mark an upstream down (until `until`) or back up (`None`).
    pub fn mark_upstream(&mut self, upstream: &str, until: Option<DateTime<Utc>>) {
        match until {
            Some(t) => {
                self.stale.insert(upstream.to_string(), t);
            }
            None => {
                self.stale.remove(upstream);
            }
        }
    }

    pub fn stale_upstreams(&self, now: DateTime<Utc>) -> Vec<String> {
        self.stale
            .iter()
            .filter(|(_, until)| **until > now)
            .map(|(u, _)| u.clone())
            .collect()
    }

    pub fn stale_upstream(&self, now: DateTime<Utc>) -> Option<&str> {
        self.stale
            .iter()
            .find(|(_, until)| **until > now)
            .map(|(u, _)| u.as_str())
    }

    /// Replaces the external disaster signals (PetaBencana) with the latest window: new ids are
    /// created, ids missing from the window are resolved. Signals keep their upstream ids.
    pub fn sync_external_disasters(&mut self, latest: Vec<DisasterSignal>, now: DateTime<Utc>) {
        let ids: std::collections::HashSet<&str> = latest.iter().map(|d| d.id.as_str()).collect();
        let gone: Vec<String> = self
            .disasters
            .iter()
            .filter(|d| !ids.contains(d.id.as_str()))
            .map(|d| d.id.clone())
            .collect();
        for id in gone {
            self.resolve_disaster(&id, now);
        }
        for d in latest {
            if self.disasters.iter().any(|x| x.id == d.id) {
                continue;
            }
            let ev = DomainEvent::SinyalBencanaMuncul {
                id: d.id.clone(),
                kind: d.kind,
            };
            self.disasters.push(d);
            self.emit(ev, now);
        }
    }

    // ---- Peringatan: subscriptions ----

    pub fn add_subscription(&mut self, sub: Subscription) {
        self.watches.push(Watch::new(sub, &self.risk));
    }

    pub fn remove_subscription(&mut self, id: &str) {
        self.watches.retain(|w| w.sub.id != id);
    }

    /// Current worst risk level inside a subscribed area.
    pub fn subscription_level(&self, id: &str) -> Option<crate::risk::RiskLevel> {
        self.watches
            .iter()
            .find(|w| w.sub.id == id)
            .map(|w| w.level)
    }

    fn check_watches(&mut self, now: DateTime<Utc>) {
        for w in &mut self.watches {
            if let Some(a) = w.check(&self.risk, now) {
                self.outbox.push(PersistOp::Alert(a));
            }
        }
    }

    pub fn take_outbox(&mut self) -> Vec<PersistOp> {
        std::mem::take(&mut self.outbox)
    }

    /// Rebuilds the stored, user-made part of the world at startup.
    pub fn restore(&mut self, data: Loaded) {
        self.next_report = self.next_report.max(data.max_report_seq);
        for (user, rep) in data.reputation {
            self.verification.set_reputation(&user, rep);
        }
        for (report, user, vote, weight) in data.votes {
            self.verification.restore_vote(&report, &user, vote, weight);
        }
        self.reports.extend(data.live);
        for r in data.archive.into_iter().rev() {
            self.archive_report(r);
        }
        for sub in data.subscriptions {
            self.add_subscription(sub);
        }
    }

    /// Drops reports, disasters and votes; keeps the change feed so `seq` never goes back.
    pub fn reset_inputs(&mut self, now: DateTime<Utc>) {
        self.reports.clear();
        self.archive.clear();
        self.disasters.clear();
        self.resolved.clear();
        self.verification.clear();
        self.stale.clear();
        self.outbox.push(PersistOp::DeleteAllReports);
        self.last_transition_at = now;
    }

    // ---- Risiko Wilayah ----

    fn recompute_risk(&mut self, now: DateTime<Utc>) {
        let next = RiskGrid::compute(&self.reports, &self.disasters, &self.weather, now);
        for (cell, from, to) in self.risk.transitions(&next) {
            self.emit(DomainEvent::TingkatRisikoBerubah { cell, from, to }, now);
        }
        self.risk = next;
        self.check_watches(now);
    }

    /// Applies time-driven rules after any change: expiry, then risk.
    pub fn settle(&mut self, now: DateTime<Utc>) {
        self.expire_due(now);
        self.recompute_risk(now);
    }

    fn snapshot(&self, now: DateTime<Utc>) -> Snapshot {
        let mut reports: Vec<Report> = self.reports.iter().rev().cloned().collect();
        reports.extend(self.archive.iter().cloned());
        let mut disasters = self.disasters.clone();
        disasters.extend(self.resolved.iter().cloned());
        Snapshot {
            seq: self.feed.last_seq(),
            updated_at: self.updated_at,
            source: self.source,
            stale: self.stale_upstream(now).is_some(),
            risk: self.risk.clone(),
            weather: self.weather.clone(),
            reports,
            disasters,
            changes: self.feed.to_vec(),
            mock: None,
        }
    }
}

/// What readers see. Immutable; replaced as a whole after every change.
pub struct Snapshot {
    pub seq: u64,
    pub updated_at: DateTime<Utc>,
    pub source: Source,
    pub stale: bool,
    pub risk: RiskGrid,
    pub weather: WeatherState,
    /// Live reports newest first, then expired ones newest first.
    pub reports: Vec<Report>,
    /// Active signals, then recently resolved ones.
    pub disasters: Vec<DisasterSignal>,
    pub changes: Vec<Change>,
    /// Generator counters as of this snapshot (mock mode only). `/_mock/state` reads this.
    pub mock: Option<MockState>,
}

/// The single writer state: the world plus, in mock mode, the generator that drives it.
/// Both live behind one mutex so there is exactly one lock to take (no lock-order bugs).
struct Inner {
    world: World,
    mock: Option<Generator>,
}

pub struct Engine {
    inner: Mutex<Inner>,
    snapshot: RwLock<Arc<Snapshot>>,
    mock_enabled: bool,
    /// Where stored changes go (the SQLite writer task). Unset in most unit tests.
    persist: OnceLock<tokio::sync::mpsc::UnboundedSender<PersistOp>>,
}

impl Engine {
    pub fn new(config: &Config, now: DateTime<Utc>) -> Self {
        let mock_cfg = config.mock.enabled.then_some(&config.mock);
        let source = if mock_cfg.is_some() {
            Source::Mock
        } else {
            Source::Live
        };
        let mut world = World::new(source, mock_cfg.map(|m| m.time_scale), now);
        let mock = mock_cfg.map(|m| Generator::new(m, &mut world, now));
        world.settle(now);
        let inner = Inner { world, mock };
        let snap = Arc::new(inner.snapshot(now));
        Self {
            inner: Mutex::new(inner),
            snapshot: RwLock::new(snap),
            mock_enabled: mock_cfg.is_some(),
            persist: OnceLock::new(),
        }
    }

    /// Loads stored data into the world and starts sending changes to `tx`.
    pub fn attach_store(
        &self,
        loaded: Loaded,
        tx: tokio::sync::mpsc::UnboundedSender<PersistOp>,
        now: DateTime<Utc>,
    ) {
        let _ = self.persist.set(tx);
        self.write(now, |i| {
            i.world.restore(loaded);
            // Expiry while we were down is stored when settle runs; nothing else to send.
        });
    }

    pub fn mock_enabled(&self) -> bool {
        self.mock_enabled
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn write<R>(&self, now: DateTime<Utc>, f: impl FnOnce(&mut Inner) -> R) -> R {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let out = f(&mut inner);
        inner.world.settle(now);
        let ops = inner.world.take_outbox();
        if let Some(tx) = self.persist.get() {
            for op in ops {
                let _ = tx.send(op);
            }
        }
        let snap = Arc::new(inner.snapshot(now));
        *self.snapshot.write().unwrap_or_else(|e| e.into_inner()) = snap;
        out
    }

    /// Reads the live world under the writer lock without publishing anything. Keep `f` tiny.
    pub fn with_world<R>(&self, f: impl FnOnce(&World) -> R) -> R {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        f(&inner.world)
    }

    /// Runs `f` as the single writer, settles time-driven rules and publishes a new snapshot.
    pub fn update<R>(&self, now: DateTime<Utc>, f: impl FnOnce(&mut World) -> R) -> R {
        self.write(now, |i| f(&mut i.world))
    }

    /// Like `update`, with the mock generator too. `None` when MOCK_MODE is off.
    pub fn update_mock<R>(
        &self,
        now: DateTime<Utc>,
        f: impl FnOnce(&mut Generator, &mut World) -> R,
    ) -> Option<R> {
        if !self.mock_enabled {
            return None;
        }
        self.write(now, |i| {
            let Inner { world, mock } = i;
            mock.as_mut().map(|g| f(g, world))
        })
    }

    /// One scheduler tick: the mock generator acts (if enabled), then rules settle.
    pub fn tick(&self, now: DateTime<Utc>) {
        self.write(now, |i| {
            if let Some(g) = i.mock.as_mut() {
                g.tick(&mut i.world, now);
            }
        })
    }
}

impl Inner {
    fn snapshot(&self, now: DateTime<Utc>) -> Snapshot {
        let mut s = self.world.snapshot(now);
        s.mock = self.mock.as_ref().map(|g| g.state(&self.world, now));
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vote_on_unknown_report_is_not_found() {
        let cfg = Config::from_lookup(|_| None).unwrap();
        let t0 = "2026-11-22T00:00:00Z".parse().unwrap();
        let e = Engine::new(&cfg, t0);
        let r = e.update(t0, |w| w.vote("rpt_404", "user_01", Vote::Confirm, t0));
        assert_eq!(r.unwrap_err(), VoteError::NotFound);
    }
}
