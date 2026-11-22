//! Rute Aman: scores candidate routes (RuteKandidat) against the current risk grid and picks
//! the recommended one (RuteDisarankan). Pure functions; the candidates come from the OSRM adapter.

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::reports::{Category, Report, ReportStatus};
use crate::risk::grid::{CellId, distance_m};
use crate::risk::{RiskGrid, RiskLevel};
use crate::rules::*;

#[derive(Clone, Debug, PartialEq)]
pub struct CandidateRoute {
    pub distance_m: f64,
    pub duration_s: f64,
    /// GeoJSON order: (lon, lat).
    pub coords: Vec<(f64, f64)>,
}

#[derive(Debug, Serialize)]
pub struct LineString {
    #[serde(rename = "type")]
    kind: &'static str,
    coordinates: Vec<[f64; 2]>,
}

#[derive(Debug, Serialize)]
pub struct RouteView {
    pub id: String,
    pub distance_m: f64,
    pub duration_s: f64,
    /// Length-weighted mean risk score along the route (PenaltiRisiko).
    pub risk_penalty: f64,
    pub max_risk_level: RiskLevel,
    pub geometry: LineString,
    pub reasons: Vec<String>,
    #[serde(skip)]
    cost: f64,
}

#[derive(Debug, Serialize)]
pub struct SafeRouteResult {
    pub recommended_id: String,
    pub routes: Vec<RouteView>,
}

pub fn polyline_length_m(coords: &[(f64, f64)]) -> f64 {
    coords
        .windows(2)
        .map(|w| distance_m((w[0].1, w[0].0), (w[1].1, w[1].0)))
        .sum()
}

/// Length of the route inside each cell, in order of first entry. Points outside the service
/// area are skipped (they carry no risk data).
fn cells_along(coords: &[(f64, f64)]) -> Vec<(CellId, f64)> {
    let mut order: Vec<CellId> = Vec::new();
    let mut len: BTreeMap<(u32, u32), f64> = BTreeMap::new();
    for w in coords.windows(2) {
        let (a, b) = ((w[0].1, w[0].0), (w[1].1, w[1].0));
        let seg = distance_m(a, b);
        let steps = (seg / ROUTE_SAMPLE_M).ceil().max(1.0) as usize;
        let piece = seg / steps as f64;
        for i in 0..steps {
            let t = (i as f64 + 0.5) / steps as f64;
            let (lat, lon) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            if let Some(c) = CellId::of(lat, lon) {
                let e = len.entry((c.row, c.col)).or_insert_with(|| {
                    order.push(c);
                    0.0
                });
                *e += piece;
            }
        }
    }
    order
        .into_iter()
        .map(|c| (c, len[&(c.row, c.col)]))
        .collect()
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Scores each candidate and picks the lowest cost. `None` when there are no candidates.
pub fn score(
    candidates: Vec<CandidateRoute>,
    risk: &RiskGrid,
    reports: &[Report],
    now: DateTime<Utc>,
) -> Option<SafeRouteResult> {
    if candidates.is_empty() {
        return None;
    }
    let fastest = candidates
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.duration_s.total_cmp(&b.1.duration_s))
        .map(|(i, _)| i)?;

    let mut routes: Vec<RouteView> = candidates
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let cells = cells_along(&c.coords);
            let total: f64 = cells.iter().map(|(_, l)| l).sum::<f64>().max(1.0);
            let mut weighted = 0.0;
            let (mut caution, mut danger, mut disasters) = (0usize, 0usize, 0usize);
            let mut max_level = RiskLevel::Safe;
            let mut max_rain: f64 = 0.0;
            for (cell, l) in &cells {
                let st = risk.get(*cell);
                weighted += st.score * l;
                max_level = max_level.max(st.level);
                max_rain = max_rain.max(st.rain_mm_h);
                match st.level {
                    RiskLevel::Caution => caution += 1,
                    RiskLevel::Danger => danger += 1,
                    RiskLevel::Safe => {}
                }
                if st.disaster_signals > 0 {
                    disasters += 1;
                }
            }
            let risk_penalty = weighted / total;
            let cost = c.duration_s * (1.0 + ROUTE_RISK_WEIGHT * risk_penalty)
                + caution as f64 * ROUTE_CAUTION_CELL_PENALTY_S
                + danger as f64 * ROUTE_DANGER_CELL_PENALTY_S;

            let on_route: HashSet<CellId> = cells.iter().map(|(c, _)| *c).collect();
            let mut by_cat: BTreeMap<&'static str, usize> = BTreeMap::new();
            for r in reports.iter().filter(|r| {
                r.status == ReportStatus::Verified
                    && r.expires_at > now
                    && on_route.contains(&r.cell)
            }) {
                *by_cat.entry(label(r.category)).or_default() += 1;
            }

            let mut reasons = Vec::new();
            if i == fastest {
                reasons.push("Rute tercepat".to_string());
            }
            if danger > 0 {
                reasons.push(format!("Melewati {danger} area Bahaya"));
            }
            if caution > 0 {
                reasons.push(format!("Melewati {caution} area Waspada"));
            }
            for (cat, n) in &by_cat {
                reasons.push(format!("{n} laporan {cat} terverifikasi di jalur"));
            }
            if max_rain >= 10.0 {
                reasons.push(format!("Hujan lebat hingga {max_rain:.0} mm/jam di jalur"));
            }
            if disasters > 0 {
                reasons.push("Dekat sinyal bencana dari PetaBencana".to_string());
            }
            if danger == 0 && caution == 0 && by_cat.is_empty() {
                reasons.push("Tidak ada risiko tercatat di jalur ini".to_string());
            }
            RouteView {
                id: format!("r{}", i + 1),
                distance_m: c.distance_m.round(),
                duration_s: c.duration_s.round(),
                risk_penalty: round2(risk_penalty),
                max_risk_level: max_level,
                geometry: LineString {
                    kind: "LineString",
                    coordinates: c.coords.iter().map(|(lon, lat)| [*lon, *lat]).collect(),
                },
                reasons,
                cost,
            }
        })
        .collect();

    let best = routes
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.cost.total_cmp(&b.1.cost))
        .map(|(i, _)| i)?;
    if best != fastest {
        let extra_min = ((routes[best].duration_s - routes[fastest].duration_s) / 60.0)
            .round()
            .max(0.0);
        let msg = if extra_min >= 1.0 {
            format!("Lebih aman dari rute tercepat (+{extra_min:.0} menit)")
        } else {
            "Lebih aman dari rute tercepat".to_string()
        };
        routes[best].reasons.insert(0, msg);
    }
    let recommended_id = routes[best].id.clone();
    // Recommended first, then the rest in OSRM order.
    let rec = routes.remove(best);
    routes.insert(0, rec);
    Some(SafeRouteResult {
        recommended_id,
        routes,
    })
}

fn label(c: Category) -> &'static str {
    match c {
        Category::Flood => "genangan",
        Category::Accident => "kecelakaan",
        Category::RoadDamage => "jalan rusak",
        Category::RoadClosure => "penutupan jalan",
        Category::EventCrowd => "keramaian",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::Origin;
    use crate::risk::inputs::WeatherState;
    use chrono::Duration;

    fn line(points: &[(f64, f64)], speed: f64) -> CandidateRoute {
        let coords: Vec<(f64, f64)> = points.iter().map(|(lat, lon)| (*lon, *lat)).collect();
        let d = polyline_length_m(&coords);
        CandidateRoute {
            distance_m: d,
            duration_s: d / speed,
            coords,
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
            confirm_count: 5,
            dispute_count: 0,
            created_at: now,
            verified_at: Some(now),
            expires_at: now + Duration::hours(3),
            reporter: "u".into(),
            cell: CellId::of(lat, lon).unwrap(),
            origin: Origin::Mock,
        }
    }

    #[test]
    fn avoids_danger_even_if_slower() {
        let now = Utc::now();
        // Fast route goes straight through a flooded spot; the slow one goes around it.
        let fast = line(&[(-6.90, 107.60), (-6.90, 107.62)], 10.0);
        let slow = line(&[(-6.90, 107.60), (-6.88, 107.61), (-6.90, 107.62)], 10.0);
        let flood = [verified(-6.90, 107.61, now), verified(-6.90, 107.61, now)];
        let grid = RiskGrid::compute(&flood, &[], &WeatherState::unavailable(now), now);
        let r = score(vec![fast, slow], &grid, &flood, now).unwrap();
        assert_eq!(r.recommended_id, "r2");
        assert_eq!(r.routes[0].id, "r2", "recommended comes first");
        assert_eq!(r.routes[0].max_risk_level, RiskLevel::Safe);
        assert!(r.routes[0].reasons[0].starts_with("Lebih aman"));
        let fast_view = &r.routes[1];
        assert_eq!(fast_view.max_risk_level, RiskLevel::Danger);
        assert!(fast_view.risk_penalty > 0.0);
        assert!(fast_view.reasons.iter().any(|s| s.contains("genangan")));
    }

    #[test]
    fn calm_city_picks_fastest() {
        let now = Utc::now();
        let grid = RiskGrid::compute(&[], &[], &WeatherState::unavailable(now), now);
        let a = line(&[(-6.90, 107.60), (-6.90, 107.62)], 10.0);
        let b = line(&[(-6.90, 107.60), (-6.88, 107.61), (-6.90, 107.62)], 10.0);
        let r = score(vec![b, a], &grid, &[], now).unwrap();
        assert_eq!(r.recommended_id, "r2");
        assert_eq!(r.routes[0].reasons[0], "Rute tercepat");
        assert_eq!(r.routes[0].risk_penalty, 0.0);
        assert!(score(vec![], &grid, &[], now).is_none());
    }

    #[test]
    fn cells_along_sums_to_length() {
        let coords = vec![(107.60, -6.90), (107.62, -6.90)];
        let cells = cells_along(&coords);
        let sum: f64 = cells.iter().map(|(_, l)| l).sum();
        assert!((sum - polyline_length_m(&coords)).abs() < 1.0);
        assert!(cells.len() >= 4);
    }
}
