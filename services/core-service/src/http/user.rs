//! App-user endpoints (JWT): create report, vote, subscriptions, alerts.
//! Handlers validate input and call the engine or the store; domain rules stay in the contexts.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query as AxQuery, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::auth::User;
use super::envelope::{Envelope, Meta};
use super::error::{ApiError, ErrorCode};
use super::params::{self, Query};
use crate::AppState;
use crate::alerts::{Alert, Subscription, subscription_limit_reached};
use crate::reports::{Category, NewReport, Origin, Report, clean_description};
use crate::risk::RiskLevel;
use crate::risk::grid::CellId;
use crate::rules::MAX_REPORTS_PER_USER_PER_HOUR;
use crate::store;
use crate::verification::{Vote, VoteError};

fn meta(st: &AppState) -> Meta {
    let s = st.engine.snapshot();
    Meta {
        seq: s.seq,
        updated_at: s.updated_at,
        source: s.source,
        stale: s.stale,
        reset: None,
    }
}

fn invalid(msg: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::ValidationError, msg)
}

fn json_body<T>(b: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    b.map(|Json(v)| v)
        .map_err(|e| invalid(format!("Body tidak valid: {}", e.body_text())))
}

fn in_area(lat: f64, lon: f64) -> Result<CellId, ApiError> {
    if !(lat.is_finite() && lon.is_finite()) {
        return Err(invalid("lat dan lon harus angka"));
    }
    CellId::of(lat, lon).ok_or_else(|| invalid("Lokasi di luar wilayah layanan (Bandung Raya)"))
}

// ---- Reports ----

#[derive(Deserialize)]
pub struct ReportCreate {
    category: String,
    lat: f64,
    lon: f64,
    description: Option<String>,
    severity: Option<u8>,
}

pub async fn create_report(
    State(st): State<AppState>,
    user: User,
    b: Result<Json<ReportCreate>, JsonRejection>,
) -> Result<(StatusCode, Json<Envelope<Report>>), ApiError> {
    let req = json_body(b)?;
    let category = Category::parse(&req.category).ok_or_else(|| {
        invalid("category harus flood, accident, road_damage, road_closure, atau event_crowd")
    })?;
    in_area(req.lat, req.lon)?;
    let description = clean_description(req.description);
    let new = NewReport {
        category,
        lat: req.lat,
        lon: req.lon,
        description,
        severity: req.severity.unwrap_or(2),
        reporter: user.id.clone(),
        origin: Origin::User,
    };
    let now = Utc::now();
    let report = st.engine.update(now, |w| {
        if w.recent_reports_by(&user.id, now) >= MAX_REPORTS_PER_USER_PER_HOUR {
            return Err(ApiError::new(
                ErrorCode::RateLimited,
                format!("Maksimal {MAX_REPORTS_PER_USER_PER_HOUR} laporan per jam"),
            )
            .with_retry_after(600));
        }
        w.create_report(new, now).map_err(invalid)
    })?;
    // Reply only once the report is in SQLite (a redeploy right after must not lose it).
    st.engine.flush().await.map_err(ApiError::db)?;
    tracing::info!(report_id = report.id, user_id = user.id, category = ?report.category, "report created");
    Ok((
        StatusCode::CREATED,
        Json(Envelope {
            data: report,
            meta: meta(&st),
        }),
    ))
}

#[derive(Deserialize)]
pub struct VoteRequest {
    vote: String,
}

pub async fn vote(
    State(st): State<AppState>,
    user: User,
    Path(id): Path<String>,
    b: Result<Json<VoteRequest>, JsonRejection>,
) -> Result<Json<Envelope<Report>>, ApiError> {
    let req = json_body(b)?;
    let vote = Vote::parse(&req.vote).ok_or_else(|| invalid("vote harus confirm atau dispute"))?;
    let now = Utc::now();
    let report = st
        .engine
        .update(now, |w| w.vote(&id, &user.id, vote, now))
        .map_err(|e| match e {
            VoteError::NotFound => ApiError::new(
                ErrorCode::NotFound,
                "Laporan tidak ditemukan atau sudah kedaluwarsa",
            ),
            VoteError::OwnReport => ApiError::new(
                ErrorCode::Forbidden,
                "Tidak bisa memberi suara pada laporan sendiri",
            ),
            VoteError::AlreadyVoted => ApiError::new(
                ErrorCode::Conflict,
                "Kamu sudah memberi suara pada laporan ini",
            ),
            VoteError::NotWaiting => ApiError::new(
                ErrorCode::Conflict,
                "Laporan sudah tidak menunggu verifikasi",
            ),
        })?;
    // Reply only once the vote is in SQLite.
    st.engine.flush().await.map_err(ApiError::db)?;
    tracing::info!(report_id = report.id, user_id = user.id, vote = vote.as_str(), status = ?report.status, "vote recorded");
    Ok(Json(Envelope {
        data: report,
        meta: meta(&st),
    }))
}

// ---- Subscriptions and alerts ----

#[derive(Serialize)]
pub struct SubscriptionView {
    #[serde(flatten)]
    sub: Subscription,
    /// Worst risk level inside the area right now.
    risk_level: RiskLevel,
}

fn view(st: &AppState, sub: Subscription) -> SubscriptionView {
    let level = st
        .engine
        .with_world(|w| w.subscription_level(&sub.id))
        .unwrap_or_default();
    SubscriptionView {
        sub,
        risk_level: level,
    }
}

#[derive(Deserialize)]
pub struct SubscriptionCreate {
    name: String,
    lat: f64,
    lon: f64,
    radius_m: Option<i64>,
}

pub async fn create_subscription(
    State(st): State<AppState>,
    user: User,
    b: Result<Json<SubscriptionCreate>, JsonRejection>,
) -> Result<(StatusCode, Json<Envelope<SubscriptionView>>), ApiError> {
    let req = json_body(b)?;
    let name: String = req
        .name
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string();
    if !(1..=60).contains(&name.chars().count()) {
        return Err(invalid("name harus 1 sampai 60 karakter"));
    }
    in_area(req.lat, req.lon)?;
    let radius = req.radius_m.unwrap_or(1000);
    if !(100..=5000).contains(&radius) {
        return Err(invalid("radius_m harus 100 sampai 5000"));
    }
    let count = store::count_subscriptions(&st.db, &user.id)
        .await
        .map_err(ApiError::db)?;
    if subscription_limit_reached(count) {
        return Err(invalid(format!(
            "Maksimal {} area langganan per akun",
            crate::rules::MAX_SUBSCRIPTIONS_PER_USER
        )));
    }
    let sub = Subscription {
        id: format!("sub_{}", &uuid::Uuid::new_v4().simple().to_string()[..20]),
        name,
        lat: req.lat,
        lon: req.lon,
        radius_m: radius as u32,
        created_at: Utc::now(),
        user_id: user.id.clone(),
    };
    store::insert_subscription(&st.db, &sub)
        .await
        .map_err(ApiError::db)?;
    st.engine
        .update(Utc::now(), |w| w.add_subscription(sub.clone()));
    Ok((
        StatusCode::CREATED,
        Json(Envelope {
            data: view(&st, sub),
            meta: meta(&st),
        }),
    ))
}

pub async fn list_subscriptions(
    State(st): State<AppState>,
    user: User,
    AxQuery(q): AxQuery<Query>,
) -> Result<Json<Envelope<Vec<SubscriptionView>>>, ApiError> {
    let (limit, offset) = params::limit_offset(&q)?;
    let subs = store::list_subscriptions(&st.db, &user.id, limit, offset)
        .await
        .map_err(ApiError::db)?;
    Ok(Json(Envelope {
        data: subs.into_iter().map(|s| view(&st, s)).collect(),
        meta: meta(&st),
    }))
}

pub async fn delete_subscription(
    State(st): State<AppState>,
    user: User,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let existed = store::delete_subscription(&st.db, &user.id, &id)
        .await
        .map_err(ApiError::db)?;
    if !existed {
        return Err(ApiError::new(
            ErrorCode::NotFound,
            "Langganan tidak ditemukan",
        ));
    }
    st.engine.update(Utc::now(), |w| w.remove_subscription(&id));
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_alerts(
    State(st): State<AppState>,
    user: User,
    AxQuery(q): AxQuery<Query>,
) -> Result<Json<Envelope<Vec<Alert>>>, ApiError> {
    let (limit, offset) = params::limit_offset(&q)?;
    let sub = q.get("subscription_id").map(String::as_str);
    let alerts = store::list_alerts(&st.db, &user.id, sub, limit, offset)
        .await
        .map_err(ApiError::db)?;
    Ok(Json(Envelope {
        data: alerts,
        meta: meta(&st),
    }))
}
