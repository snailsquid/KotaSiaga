//! Every domain tunable in one place (AGENTS.md section 4: "Risk rules live in one config file").
//! Change values here only; no other module hard-codes these numbers.

use crate::reports::Category;

pub struct Area {
    pub min_lat: f64,
    pub max_lat: f64,
    pub min_lon: f64,
    pub max_lon: f64,
}

impl Area {
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        (self.min_lat..=self.max_lat).contains(&lat) && (self.min_lon..=self.max_lon).contains(&lon)
    }
}

/// Greater Bandung. Risk is computed for every cell inside; points outside get `validation_error`.
pub const SERVICE_AREA: Area = Area {
    min_lat: -7.10,
    max_lat: -6.75,
    min_lon: 107.45,
    max_lon: 107.80,
};
/// Where the mock generator places things (AGENTS.md section 7).
pub const MOCK_AREA: Area = Area {
    min_lat: -6.98,
    max_lat: -6.84,
    min_lon: 107.54,
    max_lon: 107.72,
};

pub const CELL_SIZE_M: f64 = 500.0;
/// Reference latitude for the longitude scale, so the grid stays regular.
pub const GRID_REF_LAT: f64 = -6.9;

// Risk score = reports + rain + disasters.
pub const REPORT_SEVERITY_WEIGHT: f64 = 0.15;
pub const RAIN_FULL_MM_H: f64 = 50.0;
pub const RAIN_WEIGHT: f64 = 0.4;
pub const DISASTER_WEIGHT: f64 = 0.4;
pub const CAUTION_FROM: f64 = 0.35;
pub const DANGER_FROM: f64 = 0.7;

// Verification and reputation.
pub const VERIFY_THRESHOLD: f64 = 3.0;
pub const REPUTATION_MIN: f64 = 0.2;
pub const REPUTATION_MAX: f64 = 1.0;
pub const REPUTATION_DEFAULT: f64 = 0.5;
pub const REPUTATION_ON_VERIFIED: f64 = 0.05;
pub const REPUTATION_ON_DISPUTED: f64 = -0.1;

pub fn default_expiry_s(c: Category) -> i64 {
    match c {
        Category::Flood => 3 * 3600,
        Category::Accident => 2 * 3600,
        Category::RoadClosure => 6 * 3600,
        Category::RoadDamage => 7 * 86_400,
        Category::EventCrowd => 4 * 3600,
    }
}
/// In mock mode, expiry is divided by MOCK_TIME_SCALE and capped at this many real seconds.
pub const MOCK_EXPIRY_CAP_S: i64 = 30 * 60;

pub const CHANGE_FEED_CAPACITY: usize = 500;
pub const MAX_LIVE_REPORTS_MOCK: usize = 150;
pub const ARCHIVE_CAPACITY: usize = 200;
pub const RESOLVED_DISASTERS_CAPACITY: usize = 50;
pub const MAX_BBOX_CELLS: usize = 2500;

// Alerts (Peringatan).
pub const MAX_SUBSCRIPTIONS_PER_USER: usize = 20;
/// Half the diagonal of a 500 m cell: a cell touches a circle if its center is this much outside.
pub const CELL_HALF_DIAGONAL_M: f64 = 354.0;
/// Alerts kept per user; older ones are pruned.
pub const ALERTS_KEPT_PER_USER: i64 = 200;

// Reports from app users.
/// Reports one user may create per rolling hour (429 after that).
pub const MAX_REPORTS_PER_USER_PER_HOUR: usize = 10;

// Safe routes (Rute Aman). cost = duration_s * (1 + ROUTE_RISK_WEIGHT * risk_penalty)
//                                 + per-cell penalties for caution and danger cells.
pub const ROUTE_RISK_WEIGHT: f64 = 2.0;
pub const ROUTE_CAUTION_CELL_PENALTY_S: f64 = 60.0;
pub const ROUTE_DANGER_CELL_PENALTY_S: f64 = 600.0;
/// Sampling step when walking a route through the grid.
pub const ROUTE_SAMPLE_M: f64 = 50.0;
/// Synthetic routes in mock mode assume this average speed (about 30 km/h).
pub const MOCK_ROUTE_SPEED_M_S: f64 = 8.3;
