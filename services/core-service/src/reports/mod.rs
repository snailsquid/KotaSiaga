//! Pelaporan Insiden: citizen reports. A new report is `waiting` and never counts toward risk.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::risk::grid::CellId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Flood,
    Accident,
    RoadDamage,
    RoadClosure,
    EventCrowd,
}

impl Category {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "flood" => Self::Flood,
            "accident" => Self::Accident,
            "road_damage" => Self::RoadDamage,
            "road_closure" => Self::RoadClosure,
            "event_crowd" => Self::EventCrowd,
            _ => return None,
        })
    }

    pub fn label_id(self) -> &'static str {
        match self {
            Self::Flood => "Genangan",
            Self::Accident => "Kecelakaan",
            Self::RoadDamage => "Jalan rusak",
            Self::RoadClosure => "Penutupan jalan",
            Self::EventCrowd => "Keramaian acara",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    Waiting,
    Verified,
    Expired,
}

impl ReportStatus {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "waiting" => Self::Waiting,
            "verified" => Self::Verified,
            "expired" => Self::Expired,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub id: String,
    pub category: Category,
    pub status: ReportStatus,
    pub lat: f64,
    pub lon: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub severity: u8,
    pub confirm_count: u32,
    pub dispute_count: u32,
    pub created_at: DateTime<Utc>,
    pub verified_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    #[serde(skip)]
    pub reporter: String,
    #[serde(skip)]
    pub cell: CellId,
}

pub struct NewReport {
    pub category: Category,
    pub lat: f64,
    pub lon: f64,
    pub description: Option<String>,
    pub severity: u8,
    pub reporter: String,
}

impl NewReport {
    /// Returns the report's cell, or a message (Indonesian, user-facing) explaining what is wrong.
    pub fn validate(&self) -> Result<CellId, String> {
        if !(1..=3).contains(&self.severity) {
            return Err("severity harus 1 sampai 3".into());
        }
        if self
            .description
            .as_ref()
            .is_some_and(|d| d.chars().count() > 500)
        {
            return Err("description maksimal 500 karakter".into());
        }
        CellId::of(self.lat, self.lon).ok_or_else(|| "Lokasi di luar wilayah layanan".into())
    }
}
