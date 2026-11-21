//! Peringatan & Umpan Kota: the change feed behind `/changes` and `meta.seq`.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::rules::CHANGE_FEED_CAPACITY;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ChangeType {
    #[serde(rename = "report.created")]
    ReportCreated,
    #[serde(rename = "report.verified")]
    ReportVerified,
    #[serde(rename = "report.expired")]
    ReportExpired,
    #[serde(rename = "risk.changed")]
    RiskChanged,
    #[serde(rename = "weather.updated")]
    WeatherUpdated,
    #[serde(rename = "disaster.created")]
    DisasterCreated,
    #[serde(rename = "disaster.resolved")]
    DisasterResolved,
}

#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub seq: u64,
    pub at: DateTime<Utc>,
    #[serde(rename = "type")]
    pub kind: ChangeType,
    pub entity_id: String,
    pub summary: String,
}

#[derive(Default)]
pub struct ChangeFeed {
    buf: VecDeque<Change>,
    last_seq: u64,
}

impl ChangeFeed {
    pub fn push(
        &mut self,
        at: DateTime<Utc>,
        kind: ChangeType,
        entity_id: String,
        summary: String,
    ) -> u64 {
        self.last_seq += 1;
        if self.buf.len() == CHANGE_FEED_CAPACITY {
            self.buf.pop_front();
        }
        self.buf.push_back(Change {
            seq: self.last_seq,
            at,
            kind,
            entity_id,
            summary,
        });
        self.last_seq
    }

    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    pub fn to_vec(&self) -> Vec<Change> {
        self.buf.iter().cloned().collect()
    }
}

/// Changes after `since`, oldest first. `reset` is true when the consumer missed changes
/// (older than the retained window) or is ahead of us (e.g. after a restart).
pub fn read(changes: &[Change], last_seq: u64, since: u64, limit: usize) -> (Vec<Change>, bool) {
    if since > last_seq {
        return (Vec::new(), true);
    }
    let first = changes.first().map_or(last_seq + 1, |c| c.seq);
    let reset = since < last_seq && since + 1 < first;
    let start = changes.partition_point(|c| c.seq <= since);
    (
        changes[start..].iter().take(limit).cloned().collect(),
        reset,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(n: usize) -> ChangeFeed {
        let mut f = ChangeFeed::default();
        for i in 0..n {
            f.push(
                Utc::now(),
                ChangeType::WeatherUpdated,
                "bandung".into(),
                format!("{i}"),
            );
        }
        f
    }

    #[test]
    fn returns_changes_after_since_oldest_first() {
        let f = feed(10);
        let (got, reset) = read(&f.to_vec(), f.last_seq(), 7, 50);
        assert!(!reset);
        assert_eq!(
            got.iter().map(|c| c.seq).collect::<Vec<_>>(),
            vec![8, 9, 10]
        );
        assert_eq!(read(&f.to_vec(), f.last_seq(), 0, 2).0.len(), 2);
        assert!(read(&f.to_vec(), f.last_seq(), 10, 50).0.is_empty());
    }

    #[test]
    fn keeps_last_500_and_signals_reset() {
        let f = feed(CHANGE_FEED_CAPACITY + 20);
        let all = f.to_vec();
        assert_eq!(all.len(), CHANGE_FEED_CAPACITY);
        assert_eq!(all[0].seq, 21);
        assert!(read(&all, f.last_seq(), 5, 50).1, "missed seq 6..=20");
        assert!(
            !read(&all, f.last_seq(), 20, 50).1,
            "first kept change is 21"
        );
        assert!(read(&all, f.last_seq(), 9999, 50).1, "consumer is ahead");
    }
}
