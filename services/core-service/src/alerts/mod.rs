//! Peringatan & Umpan Kota (alerts part): area subscriptions and the alerts raised when the
//! worst risk level inside a subscribed area changes. Matching runs on every risk recompute.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::risk::grid::{CellId, distance_m};
use crate::risk::{RiskGrid, RiskLevel};
use crate::rules::{CELL_HALF_DIAGONAL_M, MAX_SUBSCRIPTIONS_PER_USER};

#[derive(Clone, Debug, Serialize)]
pub struct Subscription {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub radius_m: u32,
    pub created_at: DateTime<Utc>,
    #[serde(skip)]
    pub user_id: String,
}

/// A subscription as the risk matcher sees it: the cells it covers and the last worst level.
#[derive(Clone, Debug)]
pub struct Watch {
    pub sub: Subscription,
    cells: Vec<CellId>,
    pub level: RiskLevel,
}

#[derive(Clone, Debug, Serialize)]
pub struct Alert {
    pub id: String,
    pub subscription_id: String,
    pub cell_id: String,
    pub risk_level: RiskLevel,
    pub previous_level: RiskLevel,
    pub at: DateTime<Utc>,
    pub message: String,
    #[serde(skip)]
    pub user_id: String,
}

/// Cells whose area touches the circle (center within radius + half a cell diagonal).
pub fn covered_cells(lat: f64, lon: f64, radius_m: f64) -> Vec<CellId> {
    let Some(center) = CellId::of(lat, lon) else {
        return Vec::new();
    };
    let reach = (radius_m / crate::rules::CELL_SIZE_M).ceil() as i64 + 1;
    let mut out = Vec::new();
    for dr in -reach..=reach {
        for dc in -reach..=reach {
            let (r, c) = (center.row as i64 + dr, center.col as i64 + dc);
            if r < 0
                || c < 0
                || r >= crate::risk::grid::rows() as i64
                || c >= crate::risk::grid::cols() as i64
            {
                continue;
            }
            let cell = CellId {
                row: r as u32,
                col: c as u32,
            };
            if distance_m((lat, lon), cell.center()) <= radius_m + CELL_HALF_DIAGONAL_M {
                out.push(cell);
            }
        }
    }
    out
}

fn worst(cells: &[CellId], risk: &RiskGrid) -> (RiskLevel, Option<CellId>) {
    let mut best: (RiskLevel, Option<CellId>, f64) = (RiskLevel::Safe, None, -1.0);
    for &c in cells {
        let st = risk.get(c);
        if (st.level, st.score) > (best.0, best.2) {
            best = (st.level, Some(c), st.score);
        }
    }
    (best.0, best.1)
}

impl Watch {
    pub fn new(sub: Subscription, risk: &RiskGrid) -> Self {
        let cells = covered_cells(sub.lat, sub.lon, sub.radius_m as f64);
        let level = worst(&cells, risk).0;
        Self { sub, cells, level }
    }

    /// Returns an alert when the worst level in the area changed since the last check.
    pub fn check(&mut self, risk: &RiskGrid, now: DateTime<Utc>) -> Option<Alert> {
        let (level, cell) = worst(&self.cells, risk);
        if level == self.level {
            return None;
        }
        let previous = std::mem::replace(&mut self.level, level);
        let cell = cell.or_else(|| CellId::of(self.sub.lat, self.sub.lon))?;
        let verb = if level > previous { "naik" } else { "turun" };
        Some(Alert {
            id: format!("alr_{}", &uuid::Uuid::new_v4().simple().to_string()[..20]),
            subscription_id: self.sub.id.clone(),
            cell_id: cell.to_string(),
            risk_level: level,
            previous_level: previous,
            at: now,
            message: format!(
                "{}: risiko {verb} dari {} ke {}",
                self.sub.name,
                previous.label_id(),
                level.label_id()
            ),
            user_id: self.sub.user_id.clone(),
        })
    }
}

pub fn subscription_limit_reached(count: usize) -> bool {
    count >= MAX_SUBSCRIPTIONS_PER_USER
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::{Category, Report, ReportStatus};
    use crate::risk::inputs::WeatherState;
    use chrono::Duration;

    fn sub() -> Subscription {
        Subscription {
            id: "sub_1".into(),
            name: "Rumah".into(),
            lat: -6.9,
            lon: 107.6,
            radius_m: 1000,
            created_at: Utc::now(),
            user_id: "usr_1".into(),
        }
    }

    fn verified(lat: f64, lon: f64, now: DateTime<Utc>) -> Report {
        Report {
            id: "r".into(),
            category: Category::Flood,
            status: ReportStatus::Verified,
            lat,
            lon,
            description: None,
            severity: 3,
            confirm_count: 0,
            dispute_count: 0,
            created_at: now,
            verified_at: Some(now),
            expires_at: now + Duration::hours(3),
            reporter: "u".into(),
            cell: CellId::of(lat, lon).unwrap(),
            origin: crate::reports::Origin::Mock,
        }
    }

    #[test]
    fn covers_a_disc_of_cells() {
        let cells = covered_cells(-6.9, 107.6, 1000.0);
        // pi * (1000 + 354)^2 / 500^2 ~ 23; the grid makes it a little more or less.
        assert!((15..=40).contains(&cells.len()), "{}", cells.len());
        assert!(cells.contains(&CellId::of(-6.9, 107.6).unwrap()));
        assert!(covered_cells(-6.2, 106.8, 1000.0).is_empty());
    }

    #[test]
    fn alerts_on_level_change_only() {
        let now = Utc::now();
        let dry = WeatherState::unavailable(now);
        let calm = RiskGrid::compute(&[], &[], &dry, now);
        let mut w = Watch::new(sub(), &calm);
        assert_eq!(w.level, RiskLevel::Safe);
        assert!(w.check(&calm, now).is_none());

        let r = vec![
            verified(-6.9035, 107.6035, now),
            verified(-6.9035, 107.6035, now),
        ];
        let hot = RiskGrid::compute(&r, &[], &dry, now);
        let a = w.check(&hot, now).expect("alert on rise");
        assert_eq!(
            (a.previous_level, a.risk_level),
            (RiskLevel::Safe, RiskLevel::Danger)
        );
        assert!(a.message.contains("naik"));
        assert!(w.check(&hot, now).is_none(), "no repeat while level holds");

        let far = vec![verified(-6.80, 107.75, now), verified(-6.80, 107.75, now)];
        let elsewhere = RiskGrid::compute(&far, &[], &dry, now);
        let a = w.check(&elsewhere, now).expect("alert on fall");
        assert_eq!(a.risk_level, RiskLevel::Safe);
        assert!(a.message.contains("turun"));
    }
}
