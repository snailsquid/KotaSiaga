//! Verifikasi & Reputasi: weighted votes decide a report's status; outcomes move reporter reputation.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::reports::{Report, ReportStatus};
use crate::rules::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vote {
    Confirm,
    Dispute,
}

impl Vote {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Dispute => "dispute",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "confirm" => Some(Self::Confirm),
            "dispute" => Some(Self::Dispute),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum VoteOutcome {
    Counted,
    Verified,
    /// Net disputes reached the threshold; the report ends as `expired`.
    Disputed,
}

#[derive(Debug, PartialEq, Eq)]
pub enum VoteError {
    /// No live report with that id (never existed, or already expired/archived).
    NotFound,
    NotWaiting,
    OwnReport,
    AlreadyVoted,
}

#[derive(Default)]
struct Ballot {
    voters: HashSet<String>,
    confirm_w: f64,
    dispute_w: f64,
}

#[derive(Default)]
pub struct Verification {
    reputation: HashMap<String, f64>,
    ballots: HashMap<String, Ballot>,
}

impl Verification {
    pub fn reputation(&self, user: &str) -> f64 {
        self.reputation
            .get(user)
            .copied()
            .unwrap_or(REPUTATION_DEFAULT)
    }

    pub fn set_reputation(&mut self, user: &str, value: f64) {
        self.reputation.insert(
            user.to_string(),
            value.clamp(REPUTATION_MIN, REPUTATION_MAX),
        );
    }

    fn adjust(&mut self, user: &str, delta: f64) {
        let v = self.reputation(user) + delta;
        self.set_reputation(user, v);
    }

    pub fn has_voted(&self, report_id: &str, user: &str) -> bool {
        self.ballots
            .get(report_id)
            .is_some_and(|b| b.voters.contains(user))
    }

    pub fn vote(
        &mut self,
        report: &mut Report,
        user: &str,
        vote: Vote,
        now: DateTime<Utc>,
    ) -> Result<VoteOutcome, VoteError> {
        if report.status != ReportStatus::Waiting {
            return Err(VoteError::NotWaiting);
        }
        if report.reporter == user {
            return Err(VoteError::OwnReport);
        }
        let weight = self.reputation(user);
        let ballot = self.ballots.entry(report.id.clone()).or_default();
        if !ballot.voters.insert(user.to_string()) {
            return Err(VoteError::AlreadyVoted);
        }
        match vote {
            Vote::Confirm => {
                ballot.confirm_w += weight;
                report.confirm_count += 1;
            }
            Vote::Dispute => {
                ballot.dispute_w += weight;
                report.dispute_count += 1;
            }
        }
        let net = ballot.confirm_w - ballot.dispute_w;
        if net >= VERIFY_THRESHOLD - 1e-9 {
            report.status = ReportStatus::Verified;
            report.verified_at = Some(now);
            self.ballots.remove(&report.id);
            self.adjust(&report.reporter.clone(), REPUTATION_ON_VERIFIED);
            Ok(VoteOutcome::Verified)
        } else if -net >= VERIFY_THRESHOLD - 1e-9 {
            report.status = ReportStatus::Expired;
            self.ballots.remove(&report.id);
            self.adjust(&report.reporter.clone(), REPUTATION_ON_DISPUTED);
            Ok(VoteOutcome::Disputed)
        } else {
            Ok(VoteOutcome::Counted)
        }
    }

    /// Rebuilds a ballot from a stored vote (startup). The report's counters are already stored.
    pub fn restore_vote(&mut self, report_id: &str, user: &str, vote: Vote, weight: f64) {
        let ballot = self.ballots.entry(report_id.to_string()).or_default();
        if ballot.voters.insert(user.to_string()) {
            match vote {
                Vote::Confirm => ballot.confirm_w += weight,
                Vote::Dispute => ballot.dispute_w += weight,
            }
        }
    }

    pub fn forget(&mut self, report_id: &str) {
        self.ballots.remove(report_id);
    }

    pub fn clear(&mut self) {
        self.ballots.clear();
        self.reputation.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::Category;
    use crate::risk::grid::CellId;

    fn report() -> Report {
        let now = Utc::now();
        Report {
            id: "rpt_1".into(),
            category: Category::Flood,
            status: ReportStatus::Waiting,
            lat: -6.9,
            lon: 107.6,
            description: None,
            severity: 2,
            confirm_count: 0,
            dispute_count: 0,
            created_at: now,
            verified_at: None,
            expires_at: now,
            reporter: "reporter".into(),
            cell: CellId::of(-6.9, 107.6).unwrap(),
            origin: Default::default(),
        }
    }

    #[test]
    fn verifies_when_net_confirm_weight_reaches_threshold() {
        let mut v = Verification::default();
        let mut r = report();
        for u in ["a", "b"] {
            v.set_reputation(u, 1.0);
            assert_eq!(
                v.vote(&mut r, u, Vote::Confirm, Utc::now()),
                Ok(VoteOutcome::Counted)
            );
        }
        v.set_reputation("c", 1.0);
        assert_eq!(
            v.vote(&mut r, "c", Vote::Confirm, Utc::now()),
            Ok(VoteOutcome::Verified)
        );
        assert_eq!(r.status, ReportStatus::Verified);
        assert!(r.verified_at.is_some());
        assert!(
            (v.reputation("reporter") - (REPUTATION_DEFAULT + REPUTATION_ON_VERIFIED)).abs() < 1e-9
        );
    }

    #[test]
    fn disputes_cancel_confirms_and_can_expire_the_report() {
        let mut v = Verification::default();
        let mut r = report();
        for u in ["a", "b", "c", "d"] {
            v.set_reputation(u, 1.0);
        }
        v.vote(&mut r, "a", Vote::Confirm, Utc::now()).unwrap();
        for u in ["b", "c", "d"] {
            v.vote(&mut r, u, Vote::Dispute, Utc::now()).unwrap();
        }
        assert_eq!(r.status, ReportStatus::Waiting, "net dispute is only 2.0");
        for u in ["e", "f"] {
            v.set_reputation(u, 1.0);
        }
        v.vote(&mut r, "e", Vote::Dispute, Utc::now()).unwrap();
        assert_eq!(r.status, ReportStatus::Expired);
    }

    #[test]
    fn rejects_double_and_own_votes() {
        let mut v = Verification::default();
        let mut r = report();
        v.vote(&mut r, "a", Vote::Confirm, Utc::now()).unwrap();
        assert_eq!(
            v.vote(&mut r, "a", Vote::Dispute, Utc::now()),
            Err(VoteError::AlreadyVoted)
        );
        assert_eq!(
            v.vote(&mut r, "reporter", Vote::Confirm, Utc::now()),
            Err(VoteError::OwnReport)
        );
    }

    #[test]
    fn reputation_is_clamped() {
        let mut v = Verification::default();
        v.set_reputation("x", 5.0);
        assert_eq!(v.reputation("x"), REPUTATION_MAX);
        v.set_reputation("x", -1.0);
        assert_eq!(v.reputation("x"), REPUTATION_MIN);
    }
}
