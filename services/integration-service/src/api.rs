//! HTTP API of the Layanan Integrasi, for the KotaSiaga mobile app (base `/api`).
//!
//! - Combined (new service): `/monitor-points`, `/destination-points`, `/routes/safe`,
//!   `/integration/status`.
//! - KotaSiaga pass-through so the app needs one base URL: read endpoints (with a last-good
//!   fallback cache), auth, reports, votes, subscriptions, alerts.
//!
//! Auth follows the cluster rules (AGENTS.md section 3): app users send their KotaSiaga JWT
//! (`Authorization: Bearer`), checked against KotaSiaga `/auth/me`; server-to-server callers
//! send an `X-API-Key` from this service's `API_KEYS`. Callers' keys are never forwarded:
//! this service uses its own keys for KotaSiaga, JelajahLokal and DonorKita.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::Utc;
use partner_contracts::{health, tourism};
use serde_json::{Value, json};

use crate::state::{App, KOTASIAGA, in_area};

pub type Shared = Arc<App>;

// ---- Errors (cluster error envelope) ----

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    retry_after: Option<u64>,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            retry_after: None,
        }
    }
    fn invalid(m: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "validation_error", m)
    }
    fn not_found(m: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", m)
    }
    fn unavailable(m: impl Into<String>) -> Self {
        Self {
            retry_after: Some(10),
            ..Self::new(StatusCode::SERVICE_UNAVAILABLE, "upstream_unavailable", m)
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({"error": {"code": self.code, "message": self.message,
                                    "request_id": uuid::Uuid::new_v4().to_string()}});
        let mut res = (self.status, Json(body)).into_response();
        if let Some(s) = self.retry_after {
            res.headers_mut()
                .insert("retry-after", HeaderValue::from(s));
        }
        res
    }
}

type ApiResult = Result<Response, ApiError>;

// ---- Auth ----

pub enum Caller {
    /// An `X-API-Key` from `API_KEYS` (or the admin key).
    Key(String),
    /// A KotaSiaga user JWT, confirmed by KotaSiaga.
    User(String),
}

fn hash(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

const TOKEN_FRESH: Duration = Duration::from_secs(60);
/// While KotaSiaga is down, a token it confirmed this recently still counts.
const TOKEN_GRACE: Duration = Duration::from_secs(15 * 60);

async fn authorize(app: &App, h: &HeaderMap) -> Result<Caller, ApiError> {
    if let Some(key) = h.get("x-api-key").and_then(|v| v.to_str().ok()) {
        return app
            .cfg
            .caller(key)
            .map(|n| Caller::Key(n.to_string()))
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    "invalid_api_key",
                    "API key tidak valid",
                )
            });
    }
    let Some(token) = h
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty() && t.len() < 4096)
    else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "Kirim Authorization: Bearer <token> (pengguna) atau X-API-Key (layanan)",
        ));
    };
    let k = hash(token);
    let cached = app
        .tokens
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&k)
        .map(|(t, _)| *t);
    if cached.is_some_and(|t| t.elapsed() < TOKEN_FRESH) {
        return Ok(Caller::User(token.to_string()));
    }
    let url = format!("{}/auth/me", app.cfg.kotasiaga_base_url);
    let res = app
        .client
        .send(
            KOTASIAGA,
            reqwest::Method::GET,
            &url,
            &[("Authorization", format!("Bearer {token}"))],
            None,
            true,
            &app.faults,
        )
        .await;
    match res {
        Ok(r) if r.ok() => {
            let mut m = app.tokens.lock().unwrap_or_else(|e| e.into_inner());
            if m.len() > 5000 {
                m.retain(|_, (t, _)| t.elapsed() < TOKEN_GRACE);
            }
            m.insert(
                k,
                (
                    Instant::now(),
                    r.body.get("data").cloned().unwrap_or(Value::Null),
                ),
            );
            Ok(Caller::User(token.to_string()))
        }
        Ok(r) if r.status == 401 || r.status == 403 => Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_token",
            "Token tidak valid atau kedaluwarsa",
        )),
        _ if cached.is_some_and(|t| t.elapsed() < TOKEN_GRACE) => {
            Ok(Caller::User(token.to_string()))
        }
        _ => Err(ApiError::unavailable(
            "KotaSiaga tidak dapat memeriksa token saat ini, coba lagi",
        )),
    }
}

/// Headers for a KotaSiaga call on behalf of `caller`.
fn kotasiaga_auth(app: &App, caller: &Caller) -> Result<Vec<(&'static str, String)>, ApiError> {
    match caller {
        Caller::User(t) => Ok(vec![("Authorization", format!("Bearer {t}"))]),
        Caller::Key(name) => {
            tracing::debug!(caller = %name, "API key caller, using our KotaSiaga key");
            app.cfg
                .kotasiaga_api_key
                .clone()
                .map(|k| vec![("X-API-Key", k)])
                .ok_or_else(|| {
                    ApiError::unavailable("KOTASIAGA_API_KEY belum diatur di Layanan Integrasi")
                })
        }
    }
}

// ---- Rate limit (fixed window per client IP) ----

fn client_ip(req: &Request) -> String {
    // Behind Coolify's Traefik the last X-Forwarded-For hop is the client Traefik saw.
    if let Some(ip) = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return ip.to_string();
    }
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .unwrap_or_else(|| "unknown".into())
}

async fn rate_limit(State(app): State<Shared>, req: Request, next: Next) -> Response {
    let minute = (Utc::now().timestamp() / 60) as u64;
    let ip = client_ip(&req);
    let over = {
        let mut m = app.limiter.lock().unwrap_or_else(|e| e.into_inner());
        if m.len() > 10_000 {
            m.retain(|_, (w, _)| *w == minute);
        }
        let e = m.entry(ip).or_insert((minute, 0));
        if e.0 != minute {
            *e = (minute, 0);
        }
        e.1 += 1;
        e.1 > app.cfg.rate_limit_per_minute
    };
    if over {
        let wait = 60 - (Utc::now().timestamp() % 60) as u64;
        return ApiError {
            retry_after: Some(wait.max(1)),
            ..ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "Terlalu banyak permintaan, coba lagi nanti",
            )
        }
        .into_response();
    }
    next.run(req).await
}

// ---- KotaSiaga pass-through ----

fn reply(status: u16, body: Value, retry_after: Option<String>) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut res = if body.is_null() {
        status.into_response()
    } else {
        (status, Json(body)).into_response()
    };
    if let Some(r) = retry_after.and_then(|r| HeaderValue::from_str(&r).ok()) {
        res.headers_mut().insert("retry-after", r);
    }
    res
}

/// How long a cached KotaSiaga read may stand in while KotaSiaga is down (AGENTS.md section 6).
fn max_age(path: &str) -> Duration {
    Duration::from_secs(match path {
        "/weather" => 600,
        "/risk" | "/status" => 60,
        _ => 300,
    })
}

const CACHE_ENTRIES: usize = 500;

fn mark_stale(mut body: Value) -> Value {
    if let Some(m) = body.get_mut("meta").and_then(Value::as_object_mut) {
        m.insert("stale".into(), Value::Bool(true));
    }
    body
}

/// Forwards one call to KotaSiaga. `cache_for`: a read whose last good body may stand in.
#[allow(clippy::too_many_arguments)]
async fn forward(
    app: &App,
    method: Method,
    path: &str,
    query: Option<&str>,
    headers: Vec<(&'static str, String)>,
    body: Option<Value>,
    retry: bool,
    cache: bool,
) -> ApiResult {
    let pq = match query.filter(|q| !q.is_empty()) {
        Some(q) => format!("{path}?{q}"),
        None => path.to_string(),
    };
    let url = format!("{}{pq}", app.cfg.kotasiaga_base_url);
    let m = reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET);
    let res = app
        .client
        .send(
            KOTASIAGA,
            m,
            &url,
            &headers,
            body.as_ref(),
            retry,
            &app.faults,
        )
        .await;
    match res {
        Ok(r) if r.status < 500 => {
            if r.ok() && !(r.body.is_object() || r.body.is_null()) {
                return Err(ApiError::unavailable(
                    "Jawaban KotaSiaga tidak sesuai kontrak",
                ));
            }
            if cache && r.ok() && r.body.get("data").is_some() {
                let mut c = app.cache.write().unwrap_or_else(|e| e.into_inner());
                if c.len() >= CACHE_ENTRIES
                    && !c.contains_key(&pq)
                    && let Some(oldest) = c
                        .iter()
                        .min_by_key(|(_, (_, t))| *t)
                        .map(|(k, _)| k.clone())
                {
                    c.remove(&oldest);
                }
                c.insert(pq, (r.body.clone(), Instant::now()));
            }
            Ok(reply(r.status, r.body, r.retry_after))
        }
        other => {
            if let Err(e) = &other {
                tracing::warn!(path, error = %e, "KotaSiaga call failed");
            }
            if cache {
                let hit = app
                    .cache
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&pq)
                    .cloned();
                if let Some((b, at)) = hit.filter(|(_, at)| at.elapsed() < max_age(path)) {
                    tracing::info!(
                        path,
                        age_s = at.elapsed().as_secs(),
                        "serving cached KotaSiaga data (stale)"
                    );
                    return Ok(reply(200, mark_stale(b), None));
                }
            }
            Err(ApiError::unavailable(
                "Data KotaSiaga belum tersedia, coba lagi sebentar",
            ))
        }
    }
}

async fn read_through(State(app): State<Shared>, h: HeaderMap, req: Request) -> ApiResult {
    let caller = authorize(&app, &h).await?;
    let headers = kotasiaga_auth(&app, &caller)?;
    let path = req.uri().path().trim_start_matches("/api").to_string();
    forward(
        &app,
        Method::GET,
        &path,
        req.uri().query(),
        headers,
        None,
        true,
        true,
    )
    .await
}

/// User endpoints: the caller's JWT goes to KotaSiaga as is; nothing is cached.
async fn user_call(
    app: &App,
    h: &HeaderMap,
    method: Method,
    path: &str,
    query: Option<&str>,
    body: Option<Value>,
    retry: bool,
) -> ApiResult {
    let caller = authorize(app, h).await?;
    let Caller::User(t) = caller else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_token",
            "Endpoint ini perlu token pengguna (Bearer)",
        ));
    };
    forward(
        app,
        method,
        path,
        query,
        vec![("Authorization", format!("Bearer {t}"))],
        body,
        retry,
        false,
    )
    .await
}

fn json_body(
    b: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> Result<Value, ApiError> {
    b.map(|Json(v)| v)
        .map_err(|e| ApiError::invalid(format!("Body tidak valid: {}", e.body_text())))
}

async fn auth_public(State(app): State<Shared>, req: Request) -> ApiResult {
    let path = req.uri().path().trim_start_matches("/api").to_string();
    let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024)
        .await
        .map_err(|_| ApiError::invalid("Body terlalu besar"))?;
    let body: Value = if bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&bytes).map_err(|_| ApiError::invalid("Body harus JSON"))?
    };
    // Login can be repeated safely; register, refresh (rotates the token) and logout cannot.
    let retry = path == "/auth/login";
    forward(
        &app,
        Method::POST,
        &path,
        None,
        vec![],
        Some(body),
        retry,
        false,
    )
    .await
}

async fn auth_me(State(app): State<Shared>, h: HeaderMap) -> ApiResult {
    user_call(&app, &h, Method::GET, "/auth/me", None, None, true).await
}

async fn alerts(State(app): State<Shared>, h: HeaderMap, req: Request) -> ApiResult {
    user_call(
        &app,
        &h,
        Method::GET,
        "/alerts",
        req.uri().query(),
        None,
        true,
    )
    .await
}

async fn list_subscriptions(State(app): State<Shared>, h: HeaderMap) -> ApiResult {
    user_call(&app, &h, Method::GET, "/subscriptions", None, None, true).await
}

async fn create_subscription(
    State(app): State<Shared>,
    h: HeaderMap,
    b: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> ApiResult {
    let body = json_body(b)?;
    user_call(
        &app,
        &h,
        Method::POST,
        "/subscriptions",
        None,
        Some(body),
        false,
    )
    .await
}

fn safe_id(id: &str) -> Result<&str, ApiError> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ApiError::not_found("Data tidak ditemukan"));
    }
    Ok(id)
}

async fn delete_subscription(
    State(app): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult {
    let path = format!("/subscriptions/{}", safe_id(&id)?);
    user_call(&app, &h, Method::DELETE, &path, None, None, false).await
}

async fn create_report(
    State(app): State<Shared>,
    h: HeaderMap,
    b: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> ApiResult {
    let body = json_body(b)?;
    user_call(&app, &h, Method::POST, "/reports", None, Some(body), false).await
}

async fn vote(
    State(app): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
    b: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> ApiResult {
    let body = json_body(b)?;
    let path = format!("/reports/{}/votes", safe_id(&id)?);
    user_call(&app, &h, Method::POST, &path, None, Some(body), false).await
}

// ---- Combined endpoints ----

struct ListParams {
    bbox: Option<(f64, f64, f64, f64)>,
    risk_level: Option<String>,
    limit: usize,
    offset: usize,
}

fn list_params(q: &HashMap<String, String>) -> Result<ListParams, ApiError> {
    let bbox = match q.get("bbox") {
        None => None,
        Some(s) => {
            let v: Vec<f64> = s
                .split(',')
                .map(|x| x.trim().parse::<f64>())
                .collect::<Result<_, _>>()
                .map_err(|_| ApiError::invalid("bbox harus min_lon,min_lat,max_lon,max_lat"))?;
            if v.len() != 4 || v.iter().any(|x| !x.is_finite()) || v[0] > v[2] || v[1] > v[3] {
                return Err(ApiError::invalid(
                    "bbox harus min_lon,min_lat,max_lon,max_lat",
                ));
            }
            Some((v[0], v[1], v[2], v[3]))
        }
    };
    let risk_level = match q.get("risk_level").map(String::as_str) {
        None => None,
        Some(l @ ("safe" | "caution" | "danger")) => Some(l.to_string()),
        Some(_) => {
            return Err(ApiError::invalid(
                "risk_level harus safe, caution, atau danger",
            ));
        }
    };
    let limit = match q.get("limit") {
        None => 50,
        Some(s) => s
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=200).contains(n))
            .ok_or_else(|| ApiError::invalid("limit harus 1 sampai 200"))?,
    };
    let offset = match q.get("offset") {
        None => 0,
        Some(s) => s
            .parse::<usize>()
            .map_err(|_| ApiError::invalid("offset harus bilangan bulat >= 0"))?,
    };
    Ok(ListParams {
        bbox,
        risk_level,
        limit,
        offset,
    })
}

fn inside(b: Option<(f64, f64, f64, f64)>, lat: f64, lon: f64) -> bool {
    b.is_none_or(|(x0, y0, x1, y1)| (x0..=x1).contains(&lon) && (y0..=y1).contains(&lat))
}

fn iso(t: Option<chrono::DateTime<Utc>>) -> Value {
    t.map(|t| Value::String(partner_contracts::iso(t)))
        .unwrap_or(Value::Null)
}

fn with_risk(app: &App, mut v: Value, lat: f64, lon: f64) -> Value {
    let r = app.risk_at(lat, lon);
    let o = v.as_object_mut().expect("object");
    o.insert(
        "cell_id".into(),
        r.as_ref().map(|r| json!(r.cell_id)).unwrap_or(Value::Null),
    );
    o.insert(
        "risk_level".into(),
        r.as_ref()
            .map(|r| json!(r.risk_level))
            .unwrap_or(Value::Null),
    );
    o.insert(
        "rain_mm_h".into(),
        r.as_ref()
            .map(|r| json!(r.rain_mm_h))
            .unwrap_or(Value::Null),
    );
    v
}

pub fn monitor_point(app: &App, d: &tourism::Destination) -> Value {
    with_risk(
        app,
        json!({
            "id": format!("tour_{}", d.id), "source_id": d.id, "name": d.name, "category": d.category,
            "lat": d.lat, "lon": d.lon, "updated_at": iso(d.updated_at),
        }),
        d.lat,
        d.lon,
    )
}

pub fn destination_point(app: &App, e: &health::DonationEvent) -> Value {
    with_risk(
        app,
        json!({
            "id": format!("dnr_{}", e.id), "source_id": e.id, "name": e.name, "kind": "donation_event",
            "organizer": e.organizer, "facility_id": e.facility_id, "address": e.address,
            "lat": e.lat, "lon": e.lon, "opens_at": iso(e.opens_at), "closes_at": iso(e.closes_at),
            "slots_total": e.slots_total, "slots_available": e.slots_free,
            "capacity_per_hour": e.capacity_per_hour, "status": e.status,
            "needed_blood_types": e.needed_blood_types, "updated_at": iso(e.updated_at),
        }),
        e.lat,
        e.lon,
    )
}

fn source_meta<T>(f: &crate::state::Feed<T>) -> Value {
    json!({"source": if f.mock { "mock" } else { "live" }, "stale": f.stale,
           "fetched_at": iso(f.fetched_at), "items": f.items.len()})
}

fn list_meta<T>(app: &App, f: &crate::state::Feed<T>, total: usize, name: &str) -> Value {
    json!({
        "seq": app.seq(), "updated_at": partner_contracts::iso(Utc::now()),
        "source": if f.mock { "mock" } else { "live" },
        "stale": f.stale || app.risk_is_stale(), "total": total,
        "sources": { name: source_meta(f), "kotasiaga": {"stale": app.risk_is_stale()} },
    })
}

async fn monitor_points(
    State(app): State<Shared>,
    h: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult {
    authorize(&app, &h).await?;
    let p = list_params(&q)?;
    let category = q.get("category").map(|c| c.to_lowercase());
    let f = app.tourism_feed();
    if f.fetched_at.is_none() {
        return Err(ApiError::unavailable(
            "Data destinasi wisata (JelajahLokal) belum tersedia",
        ));
    }
    let all: Vec<Value> = f
        .items
        .iter()
        .filter(|d| inside(p.bbox, d.lat, d.lon))
        .filter(|d| category.as_ref().is_none_or(|c| &d.category == c))
        .map(|d| monitor_point(&app, d))
        .filter(|v| {
            p.risk_level
                .as_ref()
                .is_none_or(|l| v["risk_level"] == json!(l))
        })
        .collect();
    let total = all.len();
    let data: Vec<Value> = all.into_iter().skip(p.offset).take(p.limit).collect();
    Ok(
        Json(json!({"data": data, "meta": list_meta(&app, &f, total, tourism::SERVICE)}))
            .into_response(),
    )
}

async fn destination_points(
    State(app): State<Shared>,
    h: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult {
    authorize(&app, &h).await?;
    let p = list_params(&q)?;
    let f = app.health_feed();
    if f.fetched_at.is_none() {
        return Err(ApiError::unavailable(
            "Data kegiatan donor (DonorKita) belum tersedia",
        ));
    }
    let all: Vec<Value> = f
        .items
        .iter()
        .filter(|e| inside(p.bbox, e.lat, e.lon))
        .map(|e| destination_point(&app, e))
        .filter(|v| {
            p.risk_level
                .as_ref()
                .is_none_or(|l| v["risk_level"] == json!(l))
        })
        .collect();
    let total = all.len();
    let data: Vec<Value> = all.into_iter().skip(p.offset).take(p.limit).collect();
    Ok(
        Json(json!({"data": data, "meta": list_meta(&app, &f, total, health::SERVICE)}))
            .into_response(),
    )
}

fn point_in(v: Option<&Value>, name: &str) -> Result<(f64, f64), ApiError> {
    let v = v.ok_or_else(|| ApiError::invalid(format!("{name} wajib diisi")))?;
    let lat = v.get("lat").and_then(Value::as_f64);
    let lon = v.get("lon").and_then(Value::as_f64);
    match (lat, lon) {
        (Some(lat), Some(lon)) if in_area(lat, lon) => Ok((lat, lon)),
        (Some(_), Some(_)) => Err(ApiError::invalid(format!(
            "{name} di luar wilayah layanan (Bandung Raya)"
        ))),
        _ => Err(ApiError::invalid(format!(
            "{name} harus berisi lat dan lon"
        ))),
    }
}

/// The destination a `destination_id` names: a DonorKita event (`dnr_…`) or a JelajahLokal
/// destination (`tour_…`), from this service's own copy of the partner feeds.
fn resolve(app: &App, id: &str) -> Option<(Value, f64, f64, bool)> {
    if let Some(raw) = id.strip_prefix("dnr_") {
        let f = app.health_feed();
        let e = f.items.iter().find(|e| e.id == raw)?;
        return Some((destination_point(app, e), e.lat, e.lon, f.stale));
    }
    let raw = id.strip_prefix("tour_")?;
    let f = app.tourism_feed();
    let d = f.items.iter().find(|d| d.id == raw)?;
    Some((monitor_point(app, d), d.lat, d.lon, f.stale))
}

async fn safe_route(
    State(app): State<Shared>,
    h: HeaderMap,
    b: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> ApiResult {
    let caller = authorize(&app, &h).await?;
    let req = json_body(b)?;
    let origin = point_in(req.get("origin"), "origin")?;
    let (dest, info, partner_stale) = match (
        req.get("destination").filter(|v| !v.is_null()),
        req.get("destination_id").filter(|v| !v.is_null()),
    ) {
        (Some(d), None) => (point_in(Some(d), "destination")?, Value::Null, false),
        (None, Some(id)) => {
            let id = id
                .as_str()
                .ok_or_else(|| ApiError::invalid("destination_id harus string"))?;
            let (info, lat, lon, stale) = resolve(&app, id).ok_or_else(|| {
                ApiError::not_found(
                    "destination_id tidak ditemukan di /destination-points atau /monitor-points",
                )
            })?;
            ((lat, lon), info, stale)
        }
        _ => {
            return Err(ApiError::invalid(
                "Isi destination atau destination_id (salah satu)",
            ));
        }
    };
    let alternatives = match req.get("alternatives") {
        None | Some(Value::Null) => 3,
        Some(v) => v
            .as_i64()
            .filter(|n| (1..=3).contains(n))
            .ok_or_else(|| ApiError::invalid("alternatives harus 1 sampai 3"))?,
    };
    let headers = kotasiaga_auth(&app, &caller)?;
    let body = json!({"origin": {"lat": origin.0, "lon": origin.1},
                      "destination": {"lat": dest.0, "lon": dest.1}, "alternatives": alternatives});
    let url = format!("{}/routes/safe", app.cfg.kotasiaga_base_url);
    let r = app
        .client
        .send(
            KOTASIAGA,
            reqwest::Method::POST,
            &url,
            &headers,
            Some(&body),
            true,
            &app.faults,
        )
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "KotaSiaga /routes/safe failed");
            ApiError::unavailable("Layanan rute KotaSiaga sedang tidak tersedia, coba lagi nanti")
        })?;
    if r.status >= 500 {
        return Err(ApiError::unavailable(
            "Layanan rute KotaSiaga sedang tidak tersedia, coba lagi nanti",
        ));
    }
    if !r.ok() {
        return Ok(reply(r.status, r.body, r.retry_after));
    }
    let mut out = r.body;
    if out
        .get("data")
        .and_then(|d| d.get("routes"))
        .and_then(Value::as_array)
        .is_none()
    {
        return Err(ApiError::unavailable(
            "Jawaban rute KotaSiaga tidak sesuai kontrak",
        ));
    }
    if let Some(d) = out.get_mut("data").and_then(Value::as_object_mut) {
        d.insert("destination".into(), info);
    }
    if partner_stale {
        out = mark_stale(out);
    }
    Ok(Json(out).into_response())
}

async fn integration_status(State(app): State<Shared>) -> Json<Value> {
    let t = app.tourism_feed();
    let h = app.health_feed();
    let up = |f_mock: bool,
              base: &Option<String>,
              stale: bool,
              fetched: Option<chrono::DateTime<Utc>>,
              items: usize,
              err: &Option<String>| {
        json!({
            "mode": if f_mock { "mock" } else { "live" }, "base_url": if f_mock { Value::Null } else { json!(base) },
            "stale": stale, "fetched_at": iso(fetched), "items": items, "last_error": err,
        })
    };
    Json(json!({
        "data": {
            "kotasiaga": {"base_url": app.cfg.kotasiaga_base_url, "risk_stale": app.risk_is_stale(),
                          "risk_points": app.risk.read().unwrap_or_else(|e| e.into_inner()).len()},
            "tourism": up(t.mock, &app.cfg.tourism.base_url, t.stale, t.fetched_at, t.items.len(), &t.last_error),
            "health": up(h.mock, &app.cfg.health.base_url, h.stale, h.fetched_at, h.items.len(), &h.last_error),
            "mock_mode": app.cfg.mock_mode,
            "upstreams_down": app.faults.down(),
        },
        "meta": {"seq": app.seq(), "updated_at": partner_contracts::iso(Utc::now()),
                 "source": if app.cfg.mock_mode { "mock" } else { "live" },
                 "stale": t.stale || h.stale || app.risk_is_stale()},
    }))
}

// ---- Demo controls (MOCK_MODE only, admin key) ----

fn admin(app: &App, h: &HeaderMap) -> Result<(), ApiError> {
    if !app.cfg.mock_mode {
        return Err(ApiError::not_found("Endpoint tidak ditemukan"));
    }
    match h
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .and_then(|k| app.cfg.caller(k))
    {
        Some("admin") => Ok(()),
        Some(_) => Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Perlu kunci admin",
        )),
        None => Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "API key tidak valid",
        )),
    }
}

fn mock_state(app: &App) -> Value {
    json!({"data": {"seed": app.cfg.mock_seed, "upstreams_down": app.faults.down(),
                    "tourism": if app.cfg.tourism.mock { "mock" } else { "live" },
                    "health": if app.cfg.health.mock { "mock" } else { "live" }},
           "meta": {"seq": app.seq(), "updated_at": partner_contracts::iso(Utc::now()), "source": "mock", "stale": !app.faults.down().is_empty()}})
}

async fn mock_get(State(app): State<Shared>, h: HeaderMap) -> ApiResult {
    admin(&app, &h)?;
    Ok(Json(mock_state(&app)).into_response())
}

async fn mock_scenario(
    State(app): State<Shared>,
    h: HeaderMap,
    b: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> ApiResult {
    admin(&app, &h)?;
    let req = json_body(b)?;
    let now = Utc::now();
    match req.get("name").and_then(Value::as_str) {
        Some("upstream_down") => {
            let up = req
                .get("upstream")
                .and_then(Value::as_str)
                .unwrap_or(tourism::SERVICE);
            if ![tourism::SERVICE, health::SERVICE, KOTASIAGA].contains(&up) {
                return Err(ApiError::invalid(
                    "upstream harus tourism, health, atau kotasiaga",
                ));
            }
            let secs = req
                .get("seconds")
                .and_then(Value::as_u64)
                .unwrap_or(30)
                .clamp(5, 600);
            app.faults.set(up, Duration::from_secs(secs));
            tracing::info!(upstream = up, secs, "mock scenario: upstream_down");
            match up {
                "tourism" => app.refresh_tourism(now).await,
                "health" => app.refresh_health(now).await,
                _ => app.refresh_risk().await,
            }
        }
        Some("recover") => {
            app.faults.clear();
            app.refresh_tourism(now).await;
            app.refresh_health(now).await;
            app.refresh_risk().await;
        }
        _ => return Err(ApiError::invalid("name harus upstream_down atau recover")),
    }
    Ok((StatusCode::ACCEPTED, Json(mock_state(&app))).into_response())
}

// ---- Router ----

async fn health_check() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn openapi() -> impl IntoResponse {
    (
        [("content-type", "application/yaml")],
        include_str!("../openapi.yaml"),
    )
}

async fn not_found() -> ApiError {
    ApiError::not_found("Endpoint tidak ditemukan")
}

pub fn router(app: Shared) -> Router {
    let api = Router::new()
        .route("/health", get(health_check))
        .route("/openapi.yaml", get(openapi))
        .route("/integration/status", get(integration_status))
        .route("/monitor-points", get(monitor_points))
        .route("/destination-points", get(destination_points))
        .route("/routes/safe", post(safe_route))
        .route("/status", get(read_through))
        .route("/risk", get(read_through))
        .route("/weather", get(read_through))
        .route("/disasters", get(read_through))
        .route("/reports", get(read_through).post(create_report))
        .route("/reports/{id}/votes", post(vote))
        .route("/changes", get(read_through))
        .route("/alerts", get(alerts))
        .route(
            "/subscriptions",
            get(list_subscriptions).post(create_subscription),
        )
        .route("/subscriptions/{id}", delete(delete_subscription))
        .route("/auth/register", post(auth_public))
        .route("/auth/login", post(auth_public))
        .route("/auth/refresh", post(auth_public))
        .route("/auth/logout", post(auth_public))
        .route("/auth/me", get(auth_me))
        .route("/_mock/scenario", post(mock_scenario))
        .route("/_mock/state", get(mock_get))
        .fallback(not_found);
    Router::new()
        .route("/health", get(health_check))
        .nest("/api", api)
        .fallback(not_found)
        .layer(middleware::from_fn_with_state(app.clone(), rate_limit))
        .layer(axum::extract::DefaultBodyLimit::max(256 * 1024))
        .with_state(app)
}
