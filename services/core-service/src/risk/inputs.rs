//! Inputs to Risiko Wilayah in domain form. Adapters (live) and the mock generator produce these;
//! neither ever writes a risk level directly.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use super::grid::distance_m;

#[derive(Clone, Debug)]
pub struct Storm {
    pub lat: f64,
    pub lon: f64,
    pub radius_m: f64,
    pub peak_mm_h: f64,
    pub ends_at: DateTime<Utc>,
}

/// KondisiCuaca as a field over the city: a city-wide base rain plus at most one storm cell.
#[derive(Clone, Debug)]
pub struct WeatherState {
    pub available: bool,
    pub observed_at: DateTime<Utc>,
    pub base_rain_mm_h: f64,
    pub storm: Option<Storm>,
    pub temperature_offset_c: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    Clear,
    Cloudy,
    Rain,
    HeavyRain,
    Storm,
}

#[derive(Debug, Serialize)]
pub struct ForecastPoint {
    pub at: DateTime<Utc>,
    pub rain_mm_h: f64,
    pub temperature_c: f64,
}

#[derive(Debug, Serialize)]
pub struct WeatherView {
    pub observed_at: DateTime<Utc>,
    pub temperature_c: f64,
    pub rain_mm_h: f64,
    pub condition: Condition,
    pub forecast_6h: Vec<ForecastPoint>,
}

impl WeatherState {
    pub fn unavailable(now: DateTime<Utc>) -> Self {
        Self {
            available: false,
            observed_at: now,
            base_rain_mm_h: 0.0,
            storm: None,
            temperature_offset_c: 0.0,
        }
    }

    fn storm_rain(storm: &Storm, lat: f64, lon: f64) -> f64 {
        let d = distance_m((lat, lon), (storm.lat, storm.lon));
        let inner = storm.radius_m * 0.5;
        if d <= inner {
            storm.peak_mm_h
        } else if d < storm.radius_m {
            storm.peak_mm_h * (storm.radius_m - d) / (storm.radius_m - inner)
        } else {
            0.0
        }
    }

    pub fn rain_at(&self, lat: f64, lon: f64) -> f64 {
        if !self.available {
            return 0.0;
        }
        let storm = self
            .storm
            .as_ref()
            .map_or(0.0, |s| Self::storm_rain(s, lat, lon));
        (self.base_rain_mm_h + storm).clamp(0.0, 60.0)
    }

    pub fn condition(rain: f64) -> Condition {
        match rain {
            r if r < 0.1 => Condition::Clear,
            r if r < 1.0 => Condition::Cloudy,
            r if r < 10.0 => Condition::Rain,
            r if r < 30.0 => Condition::HeavyRain,
            _ => Condition::Storm,
        }
    }

    fn temperature(&self, rain: f64) -> f64 {
        30.0 - rain * 0.15 + self.temperature_offset_c
    }

    pub fn view_at(&self, lat: f64, lon: f64) -> WeatherView {
        let rain = self.rain_at(lat, lon);
        let forecast_6h = (1..=6)
            .map(|h| {
                let at = self.observed_at + Duration::hours(h);
                // Base rain halves every hour; a storm counts only while it lasts.
                let base = self.base_rain_mm_h * 0.5f64.powi(h as i32);
                let storm = self
                    .storm
                    .as_ref()
                    .filter(|s| s.ends_at > at)
                    .map_or(0.0, |s| Self::storm_rain(s, lat, lon));
                let r = (base + storm).clamp(0.0, 60.0);
                ForecastPoint {
                    at,
                    rain_mm_h: round1(r),
                    temperature_c: round1(self.temperature(r)),
                }
            })
            .collect();
        WeatherView {
            observed_at: self.observed_at,
            temperature_c: round1(self.temperature(rain)),
            rain_mm_h: round1(rain),
            condition: Self::condition(rain),
            forecast_6h,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisasterKind {
    Flood,
    Earthquake,
    Fire,
    Haze,
    Wind,
    Volcano,
}

impl DisasterKind {
    pub fn label_id(self) -> &'static str {
        match self {
            Self::Flood => "banjir",
            Self::Earthquake => "gempa",
            Self::Fire => "kebakaran",
            Self::Haze => "kabut asap",
            Self::Wind => "angin kencang",
            Self::Volcano => "gunung api",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisasterStatus {
    Active,
    Resolved,
}

/// SinyalBencana (official signal, source PetaBencana).
#[derive(Clone, Debug, Serialize)]
pub struct DisasterSignal {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: DisasterKind,
    pub lat: f64,
    pub lon: f64,
    pub reported_at: DateTime<Utc>,
    pub status: DisasterStatus,
    pub resolved_at: Option<DateTime<Utc>>,
}

pub fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storm_rain_falls_off_with_distance() {
        let now = Utc::now();
        let w = WeatherState {
            available: true,
            observed_at: now,
            base_rain_mm_h: 2.0,
            storm: Some(Storm {
                lat: -6.9,
                lon: 107.6,
                radius_m: 2000.0,
                peak_mm_h: 40.0,
                ends_at: now + Duration::minutes(2),
            }),
            temperature_offset_c: 0.0,
        };
        assert_eq!(w.rain_at(-6.9, 107.6), 42.0);
        assert_eq!(w.rain_at(-6.8, 107.7), 2.0);
        let v = w.view_at(-6.9, 107.6);
        assert_eq!(v.condition, Condition::Storm);
        assert_eq!(v.forecast_6h.len(), 6);
        assert!(
            v.forecast_6h[0].rain_mm_h < 2.0,
            "storm is over by the first forecast hour"
        );
    }
}
