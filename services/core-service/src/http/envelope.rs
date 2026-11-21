//! Success envelope from AGENTS.md section 3.

use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Live,
    Mock,
}

#[derive(Debug, Clone, Serialize)]
pub struct Meta {
    pub seq: u64,
    pub updated_at: DateTime<Utc>,
    pub source: Source,
    pub stale: bool,
    /// Only set on `/changes` when `since` is older than the retained window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct Envelope<T: Serialize> {
    pub data: T,
    pub meta: Meta,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_like_the_contract() {
        let env = Envelope {
            data: serde_json::json!({}),
            meta: Meta {
                seq: 1042,
                updated_at: "2026-11-20T10:15:03Z".parse().unwrap(),
                source: Source::Mock,
                stale: false,
                reset: None,
            },
        };
        let v = serde_json::to_value(env).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"data":{},"meta":{"seq":1042,"updated_at":"2026-11-20T10:15:03Z","source":"mock","stale":false}})
        );
    }
}
