//! Write-behind persistence for the in-memory world. The engine (single writer, sync) queues
//! `PersistOp`s; one async task applies them to SQLite in order. On startup `load` rebuilds the
//! user-made part of the world. Mock-generated data is never stored.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use tokio::sync::mpsc;

use crate::alerts::{Alert, Subscription};
use crate::reports::{Category, Origin, Report, ReportStatus};
use crate::risk::grid::CellId;
use crate::rules::{ALERTS_KEPT_PER_USER, ARCHIVE_CAPACITY};
use crate::verification::Vote;

#[derive(Debug, Clone)]
pub enum PersistOp {
    UpsertReport(Report),
    Vote {
        report_id: String,
        user_id: String,
        vote: Vote,
        weight: f64,
        at: DateTime<Utc>,
    },
    Reputation {
        user_id: String,
        value: f64,
    },
    /// `reset` demo scenario: drop every stored report and vote.
    DeleteAllReports,
    Alert(Alert),
}

pub fn spawn_writer(db: SqlitePool) -> mpsc::UnboundedSender<PersistOp> {
    let (tx, mut rx) = mpsc::unbounded_channel::<PersistOp>();
    tokio::spawn(async move {
        while let Some(op) = rx.recv().await {
            if let Err(e) = apply(&db, &op).await {
                tracing::error!(error = %e, ?op, "persist failed");
            }
        }
    });
    tx
}

fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

pub async fn apply(db: &SqlitePool, op: &PersistOp) -> Result<(), sqlx::Error> {
    match op {
        PersistOp::UpsertReport(r) => {
            let seq: i64 = r.id.trim_start_matches("rpt_").parse().unwrap_or(0);
            sqlx::query(
                "INSERT INTO reports_reports (id, seq, category, status, lat, lon, description, severity, \
                 confirm_count, dispute_count, reporter, created_at, verified_at, expires_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(id) DO UPDATE SET status = excluded.status, confirm_count = excluded.confirm_count, \
                 dispute_count = excluded.dispute_count, verified_at = excluded.verified_at",
            )
            .bind(&r.id)
            .bind(seq)
            .bind(r.category.as_str())
            .bind(r.status.as_str())
            .bind(r.lat)
            .bind(r.lon)
            .bind(&r.description)
            .bind(r.severity as i64)
            .bind(r.confirm_count as i64)
            .bind(r.dispute_count as i64)
            .bind(&r.reporter)
            .bind(ts(r.created_at))
            .bind(r.verified_at.map(ts))
            .bind(ts(r.expires_at))
            .execute(db)
            .await?;
        }
        PersistOp::Vote {
            report_id,
            user_id,
            vote,
            weight,
            at,
        } => {
            sqlx::query("INSERT OR IGNORE INTO verification_votes (report_id, user_id, vote, weight, at) VALUES (?, ?, ?, ?, ?)")
                .bind(report_id)
                .bind(user_id)
                .bind(vote.as_str())
                .bind(weight)
                .bind(ts(*at))
                .execute(db)
                .await?;
        }
        PersistOp::Reputation { user_id, value } => {
            sqlx::query(
                "INSERT INTO verification_reputation (user_id, reputation) VALUES (?, ?) \
                 ON CONFLICT(user_id) DO UPDATE SET reputation = excluded.reputation",
            )
            .bind(user_id)
            .bind(value)
            .execute(db)
            .await?;
        }
        PersistOp::DeleteAllReports => {
            sqlx::query("DELETE FROM verification_votes")
                .execute(db)
                .await?;
            sqlx::query("DELETE FROM reports_reports")
                .execute(db)
                .await?;
        }
        PersistOp::Alert(a) => {
            sqlx::query(
                "INSERT INTO alerts_alerts (id, subscription_id, user_id, cell_id, risk_level, previous_level, at, message) \
                 SELECT ?, ?, ?, ?, ?, ?, ?, ? WHERE EXISTS (SELECT 1 FROM alerts_subscriptions WHERE id = ?)",
            )
            .bind(&a.id)
            .bind(&a.subscription_id)
            .bind(&a.user_id)
            .bind(&a.cell_id)
            .bind(level_str(a.risk_level))
            .bind(level_str(a.previous_level))
            .bind(ts(a.at))
            .bind(&a.message)
            .bind(&a.subscription_id)
            .execute(db)
            .await?;
            sqlx::query(
                "DELETE FROM alerts_alerts WHERE user_id = ? AND id NOT IN \
                 (SELECT id FROM alerts_alerts WHERE user_id = ? ORDER BY at DESC LIMIT ?)",
            )
            .bind(&a.user_id)
            .bind(&a.user_id)
            .bind(ALERTS_KEPT_PER_USER)
            .execute(db)
            .await?;
        }
    }
    Ok(())
}

fn level_str(l: crate::risk::RiskLevel) -> &'static str {
    match l {
        crate::risk::RiskLevel::Safe => "safe",
        crate::risk::RiskLevel::Caution => "caution",
        crate::risk::RiskLevel::Danger => "danger",
    }
}

fn parse_level(s: &str) -> crate::risk::RiskLevel {
    match s {
        "danger" => crate::risk::RiskLevel::Danger,
        "caution" => crate::risk::RiskLevel::Caution,
        _ => crate::risk::RiskLevel::Safe,
    }
}

/// The user-made part of the world, read once at startup.
#[derive(Default)]
pub struct Loaded {
    /// Live reports (not expired), oldest first.
    pub live: Vec<Report>,
    /// Most recent expired reports, newest first.
    pub archive: Vec<Report>,
    pub votes: Vec<(String, String, Vote, f64)>,
    pub reputation: Vec<(String, f64)>,
    pub subscriptions: Vec<Subscription>,
    pub max_report_seq: u64,
}

fn report_from(row: &sqlx::sqlite::SqliteRow) -> Option<Report> {
    let lat: f64 = row.get("lat");
    let lon: f64 = row.get("lon");
    Some(Report {
        id: row.get("id"),
        category: Category::parse(row.get::<&str, _>("category"))?,
        status: ReportStatus::parse(row.get::<&str, _>("status"))?,
        lat,
        lon,
        description: row.get("description"),
        severity: row.get::<i64, _>("severity") as u8,
        confirm_count: row.get::<i64, _>("confirm_count") as u32,
        dispute_count: row.get::<i64, _>("dispute_count") as u32,
        created_at: parse_ts(row.get("created_at"))?,
        verified_at: row
            .get::<Option<String>, _>("verified_at")
            .as_deref()
            .and_then(parse_ts),
        expires_at: parse_ts(row.get("expires_at"))?,
        reporter: row.get("reporter"),
        cell: CellId::of(lat, lon)?,
        origin: Origin::User,
    })
}

pub async fn load(db: &SqlitePool, now: DateTime<Utc>) -> Result<Loaded, sqlx::Error> {
    let mut out = Loaded::default();
    for row in sqlx::query("SELECT * FROM reports_reports WHERE status != 'expired' ORDER BY seq")
        .fetch_all(db)
        .await?
    {
        if let Some(r) = report_from(&row) {
            out.live.push(r);
        }
    }
    for row in sqlx::query(
        "SELECT * FROM reports_reports WHERE status = 'expired' ORDER BY seq DESC LIMIT ?",
    )
    .bind(ARCHIVE_CAPACITY as i64)
    .fetch_all(db)
    .await?
    {
        if let Some(r) = report_from(&row) {
            out.archive.push(r);
        }
    }
    out.max_report_seq = sqlx::query("SELECT COALESCE(MAX(seq), 0) AS m FROM reports_reports")
        .fetch_one(db)
        .await?
        .get::<i64, _>("m") as u64;
    for row in sqlx::query(
        "SELECT v.report_id, v.user_id, v.vote, v.weight FROM verification_votes v \
         JOIN reports_reports r ON r.id = v.report_id WHERE r.status = 'waiting'",
    )
    .fetch_all(db)
    .await?
    {
        if let Some(v) = Vote::parse(row.get("vote")) {
            out.votes.push((
                row.get("report_id"),
                row.get("user_id"),
                v,
                row.get("weight"),
            ));
        }
    }
    for row in sqlx::query("SELECT user_id, reputation FROM verification_reputation")
        .fetch_all(db)
        .await?
    {
        out.reputation
            .push((row.get("user_id"), row.get("reputation")));
    }
    out.subscriptions = sqlx::query("SELECT * FROM alerts_subscriptions ORDER BY created_at")
        .fetch_all(db)
        .await?
        .iter()
        .filter_map(subscription_from)
        .collect();
    let _ = now;
    Ok(out)
}

fn subscription_from(row: &sqlx::sqlite::SqliteRow) -> Option<Subscription> {
    Some(Subscription {
        id: row.get("id"),
        name: row.get("name"),
        lat: row.get("lat"),
        lon: row.get("lon"),
        radius_m: row.get::<i64, _>("radius_m") as u32,
        created_at: parse_ts(row.get("created_at"))?,
        user_id: row.get("user_id"),
    })
}

pub async fn insert_subscription(db: &SqlitePool, s: &Subscription) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO alerts_subscriptions (id, user_id, name, lat, lon, radius_m, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&s.id)
    .bind(&s.user_id)
    .bind(&s.name)
    .bind(s.lat)
    .bind(s.lon)
    .bind(s.radius_m as i64)
    .bind(ts(s.created_at))
    .execute(db)
    .await?;
    Ok(())
}

/// Deletes a subscription (and its alerts) if it belongs to `user_id`. Returns whether it existed.
pub async fn delete_subscription(
    db: &SqlitePool,
    user_id: &str,
    id: &str,
) -> Result<bool, sqlx::Error> {
    let mut tx = db.begin().await?;
    let n = sqlx::query("DELETE FROM alerts_subscriptions WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n > 0 {
        sqlx::query("DELETE FROM alerts_alerts WHERE subscription_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(n > 0)
}

pub async fn list_subscriptions(
    db: &SqlitePool,
    user_id: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<Subscription>, sqlx::Error> {
    Ok(sqlx::query("SELECT * FROM alerts_subscriptions WHERE user_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?")
        .bind(user_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(db)
        .await?
        .iter()
        .filter_map(subscription_from)
        .collect())
}

pub async fn count_subscriptions(db: &SqlitePool, user_id: &str) -> Result<usize, sqlx::Error> {
    Ok(
        sqlx::query("SELECT COUNT(*) AS n FROM alerts_subscriptions WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(db)
            .await?
            .get::<i64, _>("n") as usize,
    )
}

pub async fn list_alerts(
    db: &SqlitePool,
    user_id: &str,
    subscription_id: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<Vec<Alert>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT * FROM alerts_alerts WHERE user_id = ? AND (? IS NULL OR subscription_id = ?) \
         ORDER BY at DESC LIMIT ? OFFSET ?",
    )
    .bind(user_id)
    .bind(subscription_id)
    .bind(subscription_id)
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(db)
    .await?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            Some(Alert {
                id: row.get("id"),
                subscription_id: row.get("subscription_id"),
                cell_id: row.get("cell_id"),
                risk_level: parse_level(row.get("risk_level")),
                previous_level: parse_level(row.get("previous_level")),
                at: parse_ts(row.get("at"))?,
                message: row.get("message"),
                user_id: row.get("user_id"),
            })
        })
        .collect())
}
