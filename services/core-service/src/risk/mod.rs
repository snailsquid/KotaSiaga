//! Risiko Wilayah: per-cell risk from verified reports, rain and disaster signals.
//! The whole grid is recomputed when inputs change; reads only look values up.

pub mod grid;
pub mod inputs;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::reports::{Report, ReportStatus};
use crate::rules::*;
use grid::CellId;
use inputs::{DisasterSignal, DisasterStatus, WeatherState, round1};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    #[default]
    Safe,
    Caution,
    Danger,
}

impl RiskLevel {
    pub fn from_score(score: f64) -> Self {
        if score >= DANGER_FROM {
            Self::Danger
        } else if score >= CAUTION_FROM {
            Self::Caution
        } else {
            Self::Safe
        }
    }

    pub fn label_id(self) -> &'static str {
        match self {
            Self::Safe => "Aman",
            Self::Caution => "Waspada",
            Self::Danger => "Bahaya",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CellState {
    pub score: f64,
    pub level: RiskLevel,
    pub rain_mm_h: f64,
    pub verified_reports: u32,
    pub disaster_signals: u32,
}

#[derive(Clone, Debug)]
pub struct RiskGrid {
    cells: Vec<CellState>,
}

/// A verified report's contribution, decaying linearly from verification to expiry.
pub fn report_weight(r: &Report, now: DateTime<Utc>) -> f64 {
    if r.status != ReportStatus::Verified || now >= r.expires_at {
        return 0.0;
    }
    let start = r.verified_at.unwrap_or(r.created_at);
    let total = (r.expires_at - start).num_milliseconds().max(1) as f64;
    let left = (r.expires_at - now).num_milliseconds() as f64;
    r.severity as f64 * REPORT_SEVERITY_WEIGHT * (left / total).clamp(0.0, 1.0)
}

impl RiskGrid {
    pub fn empty() -> Self {
        Self {
            cells: vec![CellState::default(); grid::cell_count()],
        }
    }

    pub fn compute(
        reports: &[Report],
        disasters: &[DisasterSignal],
        weather: &WeatherState,
        now: DateTime<Utc>,
    ) -> Self {
        let mut cells = vec![CellState::default(); grid::cell_count()];
        if weather.available {
            for (i, c) in cells.iter_mut().enumerate() {
                let (lat, lon) = CellId::from_index(i).center();
                c.rain_mm_h = weather.rain_at(lat, lon);
                c.score += (c.rain_mm_h / RAIN_FULL_MM_H).min(1.0) * RAIN_WEIGHT;
            }
        }
        for r in reports {
            let w = report_weight(r, now);
            if w > 0.0 {
                let c = &mut cells[r.cell.index()];
                c.score += w;
                c.verified_reports += 1;
            }
        }
        for d in disasters
            .iter()
            .filter(|d| d.status == DisasterStatus::Active)
        {
            if let Some(cell) = CellId::of(d.lat, d.lon) {
                for n in cell.with_neighbors() {
                    let c = &mut cells[n.index()];
                    c.score += DISASTER_WEIGHT;
                    c.disaster_signals += 1;
                }
            }
        }
        for c in &mut cells {
            c.level = RiskLevel::from_score(c.score);
        }
        Self { cells }
    }

    pub fn get(&self, id: CellId) -> &CellState {
        &self.cells[id.index()]
    }

    /// Cells whose level differs between `self` (before) and `next`.
    pub fn transitions(&self, next: &RiskGrid) -> Vec<(CellId, RiskLevel, RiskLevel)> {
        self.cells
            .iter()
            .zip(&next.cells)
            .enumerate()
            .filter(|(_, (a, b))| a.level != b.level)
            .map(|(i, (a, b))| (CellId::from_index(i), a.level, b.level))
            .collect()
    }
}

#[derive(Debug, Serialize)]
pub struct Factor {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct CellRiskView {
    pub cell_id: String,
    pub risk_level: RiskLevel,
    pub score: f64,
    pub factors: [Factor; 3],
}

impl CellRiskView {
    pub fn new(id: CellId, c: &CellState) -> Self {
        Self {
            cell_id: id.to_string(),
            risk_level: c.level,
            score: (c.score * 100.0).round() / 100.0,
            factors: [
                Factor {
                    kind: "rain",
                    value: Some(round1(c.rain_mm_h)),
                    count: None,
                },
                Factor {
                    kind: "verified_report",
                    value: None,
                    count: Some(c.verified_reports),
                },
                Factor {
                    kind: "disaster_signal",
                    value: None,
                    count: Some(c.disaster_signals),
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::Category;
    use chrono::Duration;
    use inputs::DisasterKind;

    const P: (f64, f64) = (-6.9, 107.6);

    fn report(status: ReportStatus, severity: u8, now: DateTime<Utc>) -> Report {
        Report {
            id: "r".into(),
            category: Category::Flood,
            status,
            lat: P.0,
            lon: P.1,
            description: None,
            severity,
            confirm_count: 0,
            dispute_count: 0,
            created_at: now,
            verified_at: (status == ReportStatus::Verified).then_some(now),
            expires_at: now + Duration::hours(3),
            reporter: "u".into(),
            cell: CellId::of(P.0, P.1).unwrap(),
        }
    }

    fn dry(now: DateTime<Utc>) -> WeatherState {
        WeatherState {
            available: true,
            observed_at: now,
            base_rain_mm_h: 0.0,
            storm: None,
            temperature_offset_c: 0.0,
        }
    }

    fn level(g: &RiskGrid) -> RiskLevel {
        g.get(CellId::of(P.0, P.1).unwrap()).level
    }

    #[test]
    fn waiting_reports_never_raise_risk() {
        let now = Utc::now();
        let rs: Vec<_> = (0..10)
            .map(|_| report(ReportStatus::Waiting, 3, now))
            .collect();
        let g = RiskGrid::compute(&rs, &[], &dry(now), now);
        assert_eq!(g.get(CellId::of(P.0, P.1).unwrap()).score, 0.0);
    }

    #[test]
    fn two_severe_verified_reports_are_danger() {
        let now = Utc::now();
        let one = vec![report(ReportStatus::Verified, 3, now)];
        assert_eq!(
            level(&RiskGrid::compute(&one, &[], &dry(now), now)),
            RiskLevel::Caution
        );
        let two = vec![
            report(ReportStatus::Verified, 3, now),
            report(ReportStatus::Verified, 3, now),
        ];
        assert_eq!(
            level(&RiskGrid::compute(&two, &[], &dry(now), now)),
            RiskLevel::Danger
        );
    }

    #[test]
    fn verified_report_decays_to_zero_at_expiry() {
        let now = Utc::now();
        let r = report(ReportStatus::Verified, 2, now);
        assert!((report_weight(&r, now) - 0.30).abs() < 1e-9);
        assert!((report_weight(&r, now + Duration::minutes(90)) - 0.15).abs() < 1e-6);
        assert_eq!(report_weight(&r, r.expires_at), 0.0);
        assert_eq!(report_weight(&r, r.expires_at + Duration::seconds(1)), 0.0);
    }

    #[test]
    fn heavy_rain_alone_is_caution_and_disaster_spreads_to_neighbors() {
        let now = Utc::now();
        let mut w = dry(now);
        w.base_rain_mm_h = 50.0;
        assert_eq!(
            level(&RiskGrid::compute(&[], &[], &w, now)),
            RiskLevel::Caution
        );

        let d = DisasterSignal {
            id: "d".into(),
            kind: DisasterKind::Flood,
            lat: P.0,
            lon: P.1,
            reported_at: now,
            status: DisasterStatus::Active,
            resolved_at: None,
        };
        let g = RiskGrid::compute(&[], std::slice::from_ref(&d), &dry(now), now);
        let cell = CellId::of(P.0, P.1).unwrap();
        for n in cell.with_neighbors() {
            assert_eq!(g.get(n).level, RiskLevel::Caution);
        }
        let before = RiskGrid::compute(&[], &[], &dry(now), now);
        assert_eq!(before.transitions(&g).len(), 9);
    }
}
