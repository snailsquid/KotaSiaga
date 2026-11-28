use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::Query;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use http_body_util::BodyExt;
use partner_contracts::{health, tourism};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::api::router;
use crate::config::Config;
use crate::state::App;

const KS_KEY: &str = "ks-key-0123456789abcdef";
const APP_KEY: &str = "app-key-0123456789abcdef";
const ADMIN: &str = "admin-key-0123456789abcd";
const GOOD_TOKEN: &str = "good-token";
const LOGIN_OK: &str = "uji-benar";
const LOGIN_BAD: &str = "uji-salah";

type Seen = Arc<Mutex<Vec<String>>>;

async fn serve(app: Router) -> String {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let a = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    format!("http://{a}")
}

fn ks_auth(h: &HeaderMap) -> bool {
    h.get("x-api-key").is_some_and(|v| v == KS_KEY)
        || h.get("authorization")
            .is_some_and(|v| v == &format!("Bearer {GOOD_TOKEN}"))
}

fn unauthorized() -> (StatusCode, Json<Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": {"code": "invalid_api_key", "message": "x", "request_id": "r"}})),
    )
}

fn meta() -> Value {
    json!({"seq": 7, "updated_at": "2026-11-28T12:00:00Z", "source": "live", "stale": false})
}

async fn fake_kotasiaga(seen: Seen) -> String {
    let s1 = seen.clone();
    let s2 = seen.clone();
    let s3 = seen.clone();
    let app = Router::new()
        .route("/api/risk", get(move |h: HeaderMap, Query(q): Query<HashMap<String, String>>| {
            let s = s1.clone();
            async move {
                if !ks_auth(&h) {
                    return unauthorized();
                }
                s.lock().unwrap().push(format!("risk {} {}", q["lat"], q["lon"]));
                let lat: f64 = q["lat"].parse().unwrap();
                let level = if lat < -6.9 { "danger" } else { "safe" };
                (StatusCode::OK, Json(json!({"data": {"cell_id": "c_1_2", "risk_level": level, "score": 0.1,
                    "factors": [{"type": "rain", "value": 4.0}]}, "meta": meta()})))
            }
        }))
        .route("/api/weather", get(|h: HeaderMap| async move {
            if !ks_auth(&h) {
                return unauthorized();
            }
            (StatusCode::OK, Json(json!({"data": {"observed_at": "2026-11-28T12:00:00Z", "temperature_c": 27.0,
                "rain_mm_h": 1.0, "condition": "cloudy", "forecast_6h": []}, "meta": meta()})))
        }))
        .route("/api/routes/safe", post(move |h: HeaderMap, Json(b): Json<Value>| {
            let s = s2.clone();
            async move {
                if !ks_auth(&h) {
                    return unauthorized();
                }
                s.lock().unwrap().push(format!("route {}", b["destination"]));
                (StatusCode::OK, Json(json!({"data": {"recommended_id": "r1", "routes": [{"id": "r1", "distance_m": 100,
                    "duration_s": 60, "risk_penalty": 0.0, "max_risk_level": "safe",
                    "geometry": {"type": "LineString", "coordinates": []}, "reasons": ["ok"]}]}, "meta": meta()})))
            }
        }))
        .route("/api/auth/me", get(|h: HeaderMap| async move {
            if h.get("authorization").is_some_and(|v| v == &format!("Bearer {GOOD_TOKEN}")) {
                (StatusCode::OK, Json(json!({"data": {"id": "usr_1", "name": "Uji"}, "meta": meta()})))
            } else {
                (StatusCode::UNAUTHORIZED, Json(json!({"error": {"code": "invalid_token", "message": "x", "request_id": "r"}})))
            }
        }))
        .route("/api/auth/login", post(|Json(b): Json<Value>| async move {
            if b["password"] == LOGIN_OK {
                (StatusCode::OK, Json(json!({"data": {"access_token": GOOD_TOKEN}, "meta": meta()})))
            } else {
                (StatusCode::UNAUTHORIZED, Json(json!({"error": {"code": "invalid_credentials", "message": "x", "request_id": "r"}})))
            }
        }))
        .route("/api/reports", post(move |h: HeaderMap, Json(b): Json<Value>| {
            let s = s3.clone();
            async move {
                s.lock().unwrap().push(format!(
                    "report auth={} key={}",
                    h.get("authorization").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default(),
                    h.get("x-api-key").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default()
                ));
                (StatusCode::CREATED, Json(json!({"data": {"id": "rpt_1", "category": b["category"], "status": "waiting"}, "meta": meta()})))
            }
        }));
    serve(app).await
}

async fn fake_partners(seen: Seen) -> String {
    let s1 = seen.clone();
    let s2 = seen.clone();
    let app = Router::new()
        .route(
            "/api/feed/destinations",
            get(
                move |h: HeaderMap, Query(q): Query<HashMap<String, String>>| {
                    let s = s1.clone();
                    async move {
                        s.lock().unwrap().push(format!(
                            "tourism limit={} offset={} key={}",
                            q["limit"],
                            q["offset"],
                            h.get("x-api-key").unwrap().to_str().unwrap()
                        ));
                        Json(serde_json::from_str::<Value>(tourism::SAMPLE).unwrap())
                    }
                },
            ),
        )
        .route(
            "/api/donation-events",
            get(
                move |h: HeaderMap, Query(q): Query<HashMap<String, String>>| {
                    let s = s2.clone();
                    async move {
                        s.lock().unwrap().push(format!(
                            "health limit={} offset={} key={}",
                            q["limit"],
                            q["offset"],
                            h.get("x-api-key").unwrap().to_str().unwrap()
                        ));
                        Json(serde_json::from_str::<Value>(health::SAMPLE).unwrap())
                    }
                },
            ),
        );
    serve(app).await
}

struct Env {
    app: Arc<App>,
    ks_seen: Seen,
    partner_seen: Seen,
}

async fn env(extra: &[(&str, &str)], live_partners: bool) -> Env {
    let ks_seen: Seen = Arc::default();
    let partner_seen: Seen = Arc::default();
    let ks = fake_kotasiaga(ks_seen.clone()).await;
    let partners = fake_partners(partner_seen.clone()).await;
    let mut m: HashMap<String, String> = HashMap::new();
    m.insert("KOTASIAGA_BASE_URL".into(), format!("{ks}/api"));
    m.insert("KOTASIAGA_API_KEY".into(), KS_KEY.into());
    m.insert("API_KEYS".into(), format!("demo:{APP_KEY}"));
    m.insert("ADMIN_API_KEY".into(), ADMIN.into());
    m.insert("UPSTREAM_TIMEOUT_MS".into(), "2000".into());
    m.insert("MOCK_SEED".into(), "42".into());
    if live_partners {
        m.insert("TOURISM_BASE_URL".into(), format!("{partners}/api"));
        m.insert("TOURISM_API_KEY".into(), "tourism-partner-key-123".into());
        m.insert("HEALTH_BASE_URL".into(), format!("{partners}/api"));
        m.insert("HEALTH_API_KEY".into(), "health-partner-key-1234".into());
    }
    for (k, v) in extra {
        m.insert(k.to_string(), v.to_string());
    }
    let cfg = Config::from_lookup(|k| m.get(k).cloned()).unwrap();
    let app = Arc::new(App::new(cfg));
    app.refresh_tourism(Utc::now()).await;
    app.refresh_health(Utc::now()).await;
    app.refresh_risk().await;
    Env {
        app,
        ks_seen,
        partner_seen,
    }
}

async fn call(
    app: &Arc<App>,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let req = match body {
        Some(b) => req
            .header("content-type", "application/json")
            .body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let res = router(app.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

const K: (&str, &str) = ("x-api-key", APP_KEY);

#[tokio::test]
async fn health_is_public() {
    let e = env(&[], false).await;
    let (s, b) = call(&e.app, "GET", "/api/health", &[], None).await;
    assert_eq!((s, b), (StatusCode::OK, json!({"status": "ok"})));
    assert_eq!(
        call(&e.app, "GET", "/health", &[], None).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn live_partners_are_combined_with_kotasiaga_risk() {
    let e = env(&[], true).await;
    let seen = e.partner_seen.lock().unwrap().clone();
    assert!(
        seen.contains(&"tourism limit=200 offset=0 key=tourism-partner-key-123".to_string()),
        "{seen:?}"
    );
    assert!(
        seen.contains(&"health limit=200 offset=0 key=health-partner-key-1234".to_string()),
        "{seen:?}"
    );

    let (s, b) = call(&e.app, "GET", "/api/monitor-points", &[K], None).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let ids: Vec<&str> = b["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["tour_1", "tour_3"],
        "Gua Pawon is outside Bandung Raya"
    );
    let p = &b["data"][0];
    assert_eq!(p["source_id"], "1");
    assert_eq!(p["category"], "culture_history");
    assert_eq!(p["risk_level"], "danger");
    assert_eq!(p["cell_id"], "c_1_2");
    assert_eq!(p["rain_mm_h"], 4.0);
    assert_eq!(b["meta"]["source"], "live");
    assert_eq!(b["meta"]["stale"], false);
    assert_eq!(b["meta"]["total"], 2);

    let (s, b) = call(&e.app, "GET", "/api/destination-points", &[K], None).await;
    assert_eq!(s, StatusCode::OK);
    let d = &b["data"][0];
    assert_eq!(d["id"], "dnr_evt_1");
    assert_eq!(
        d["slots_available"], 47,
        "DonorKita slots_free -> slots_available"
    );
    assert_eq!(d["slots_total"], 50);
    assert_eq!(d["organizer"], "Unit Uji Kampus");
    assert_eq!(d["opens_at"], "2036-11-29T01:00:00Z");
    assert_eq!(
        d["needed_blood_types"],
        json!(["ab_negative", "o_negative"])
    );
    assert_eq!(d["risk_level"], "safe");

    let (_, b) = call(
        &e.app,
        "GET",
        "/api/monitor-points?risk_level=safe&category=nature",
        &[K],
        None,
    )
    .await;
    assert_eq!(b["data"].as_array().unwrap().len(), 1);
    assert_eq!(b["data"][0]["id"], "tour_3");
    assert_eq!(
        call(&e.app, "GET", "/api/monitor-points?limit=500", &[K], None)
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        call(&e.app, "GET", "/api/monitor-points?bbox=1,2", &[K], None)
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn auth_rules() {
    let e = env(&[], false).await;
    let (s, b) = call(&e.app, "GET", "/api/monitor-points", &[], None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(b["error"]["code"], "invalid_api_key");
    assert!(b["error"]["request_id"].is_string());
    assert_eq!(
        call(
            &e.app,
            "GET",
            "/api/monitor-points",
            &[("x-api-key", "wrong-key-0000000000")],
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let bearer = format!("Bearer {GOOD_TOKEN}");
    assert_eq!(
        call(
            &e.app,
            "GET",
            "/api/monitor-points",
            &[("authorization", &bearer)],
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    let (s, b) = call(
        &e.app,
        "GET",
        "/api/monitor-points",
        &[("authorization", "Bearer nope")],
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(b["error"]["code"], "invalid_token");
}

#[tokio::test]
async fn closed_events_are_hidden_even_from_a_stale_feed() {
    let e = env(&[], true).await;
    {
        let mut f = e.app.health.write().unwrap();
        f.items[0].closes_at = Some(Utc::now() - chrono::Duration::minutes(1));
        f.stale = true;
    }
    let (s, b) = call(&e.app, "GET", "/api/destination-points", &[K], None).await;
    assert_eq!(s, StatusCode::OK);
    let ids: Vec<&str> = b["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["dnr_evt_2"]);
    assert_eq!(b["meta"]["total"], 1);
    assert_eq!(b["meta"]["stale"], true);
    let body = json!({"origin": {"lat": -6.90, "lon": 107.60}, "destination_id": "dnr_evt_1"});
    let (s, _) = call(&e.app, "POST", "/api/routes/safe", &[K], Some(body)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let body = json!({"origin": {"lat": -6.90, "lon": 107.60}, "destination_id": "dnr_evt_2"});
    assert_eq!(
        call(&e.app, "POST", "/api/routes/safe", &[K], Some(body))
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn safe_route_by_destination_id() {
    let e = env(&[], true).await;
    let body = json!({"origin": {"lat": -6.90, "lon": 107.60}, "destination_id": "dnr_evt_1"});
    let (s, b) = call(&e.app, "POST", "/api/routes/safe", &[K], Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["recommended_id"], "r1");
    assert_eq!(b["data"]["destination"]["id"], "dnr_evt_1");
    assert_eq!(
        b["data"]["destination"]["name"],
        "Donor Darah Uji Kampus Ganesha"
    );
    let seen = e.ks_seen.lock().unwrap().clone();
    assert!(
        seen.iter()
            .any(|l| l.contains("route") && l.contains("-6.8915") && l.contains("107.6107")),
        "{seen:?}"
    );

    let tour = json!({"origin": {"lat": -6.90, "lon": 107.60}, "destination_id": "tour_3"});
    assert_eq!(
        call(&e.app, "POST", "/api/routes/safe", &[K], Some(tour))
            .await
            .0,
        StatusCode::OK
    );
    for (bad, code) in [
        (
            json!({"origin": {"lat": -6.90, "lon": 107.60}, "destination_id": "dnr_evt_99"}),
            StatusCode::NOT_FOUND,
        ),
        (
            json!({"origin": {"lat": -6.90, "lon": 107.60}}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({"origin": {"lat": -6.2, "lon": 106.8}, "destination": {"lat": -6.91, "lon": 107.62}}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({"origin": {"lat": -6.90, "lon": 107.60}, "destination": {"lat": -6.91, "lon": 107.62}, "alternatives": 9}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        assert_eq!(
            call(&e.app, "POST", "/api/routes/safe", &[K], Some(bad.clone()))
                .await
                .0,
            code,
            "{bad}"
        );
    }
}

#[tokio::test]
async fn kotasiaga_down_serves_cache_as_stale() {
    let e = env(&[], false).await;
    let (s, b) = call(&e.app, "GET", "/api/weather?lat=-6.9&lon=107.6", &[K], None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["meta"]["stale"], false);
    e.app
        .faults
        .set("kotasiaga", std::time::Duration::from_secs(30));
    let (s, b) = call(&e.app, "GET", "/api/weather?lat=-6.9&lon=107.6", &[K], None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["meta"]["stale"], true);
    assert_eq!(b["data"]["condition"], "cloudy");
    let (s, b) = call(
        &e.app,
        "GET",
        "/api/weather?lat=-6.95&lon=107.6",
        &[K],
        None,
    )
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(b["error"]["code"], "upstream_unavailable");
    e.app.refresh_risk().await;
    let (_, b) = call(&e.app, "GET", "/api/monitor-points", &[K], None).await;
    assert_eq!(b["meta"]["stale"], true);
    assert!(b["data"][0]["risk_level"].is_string());
}

#[tokio::test]
async fn mock_mode_uses_partner_shapes_and_can_take_a_partner_down() {
    let e = env(&[("MOCK_MODE", "true")], true).await;
    assert!(
        e.partner_seen.lock().unwrap().is_empty(),
        "MOCK_MODE mocks the partners"
    );
    let (s, b) = call(&e.app, "GET", "/api/monitor-points?limit=200", &[K], None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"].as_array().unwrap().len(), 11);
    assert_eq!(b["meta"]["source"], "mock");
    let (_, d) = call(&e.app, "GET", "/api/destination-points", &[K], None).await;
    assert_eq!(d["data"].as_array().unwrap().len(), 6);
    assert!(d["data"][0]["slots_available"].is_u64());

    let seq = b["meta"]["seq"].as_u64().unwrap();
    let (s, st) = call(
        &e.app,
        "POST",
        "/api/_mock/scenario",
        &[("x-api-key", ADMIN)],
        Some(json!({"name": "upstream_down", "upstream": "tourism"})),
    )
    .await;
    assert_eq!(s, StatusCode::ACCEPTED, "{st}");
    assert_eq!(st["data"]["upstreams_down"], json!(["tourism"]));
    let (_, b) = call(&e.app, "GET", "/api/monitor-points?limit=200", &[K], None).await;
    assert_eq!(b["meta"]["stale"], true);
    assert_eq!(b["meta"]["sources"]["tourism"]["stale"], true);
    assert_eq!(b["data"].as_array().unwrap().len(), 11, "last good copy");
    assert!(b["meta"]["seq"].as_u64().unwrap() > seq);
    let (_, st) = call(&e.app, "GET", "/api/integration/status", &[], None).await;
    assert_eq!(st["data"]["tourism"]["stale"], true);
    assert_eq!(st["data"]["tourism"]["mode"], "mock");
    assert_eq!(st["data"]["tourism"]["last_error"], "simulated_down");
    assert!(st["data"]["tourism"].get("base_url").is_none());
    assert!(st["data"]["kotasiaga"].get("base_url").is_none());

    call(
        &e.app,
        "POST",
        "/api/_mock/scenario",
        &[("x-api-key", ADMIN)],
        Some(json!({"name": "recover"})),
    )
    .await;
    let (_, b) = call(&e.app, "GET", "/api/monitor-points", &[K], None).await;
    assert_eq!(b["meta"]["stale"], false);
    assert_eq!(
        call(
            &e.app,
            "POST",
            "/api/_mock/scenario",
            &[K],
            Some(json!({"name": "recover"}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn mock_controls_are_404_outside_mock_mode() {
    let e = env(&[], false).await;
    let (s, _) = call(
        &e.app,
        "POST",
        "/api/_mock/scenario",
        &[("x-api-key", ADMIN)],
        Some(json!({"name": "recover"})),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn user_calls_forward_the_jwt_never_a_key() {
    let e = env(&[], false).await;
    let (s, b) = call(
        &e.app,
        "POST",
        "/api/auth/login",
        &[],
        Some(json!({"email": "a@b.test", "password": LOGIN_BAD})),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(b["error"]["code"], "invalid_credentials");
    let (s, b) = call(
        &e.app,
        "POST",
        "/api/auth/login",
        &[],
        Some(json!({"email": "a@b.test", "password": LOGIN_OK})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let token = b["data"]["access_token"].as_str().unwrap().to_string();
    let bearer = format!("Bearer {token}");
    let (s, b) = call(
        &e.app,
        "POST",
        "/api/reports",
        &[("authorization", &bearer)],
        Some(json!({"category": "flood", "lat": -6.9, "lon": 107.6})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let seen = e.ks_seen.lock().unwrap().clone();
    assert!(
        seen.contains(&format!("report auth=Bearer {GOOD_TOKEN} key=")),
        "{seen:?}"
    );
    let (s, _) = call(
        &e.app,
        "POST",
        "/api/reports",
        &[K],
        Some(json!({"category": "flood"})),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rate_limited_per_client() {
    let e = env(&[("RATE_LIMIT_PER_MINUTE", "10")], false).await;
    for _ in 0..10 {
        let (s, _) = call(
            &e.app,
            "GET",
            "/api/health",
            &[("x-forwarded-for", "10.0.0.1")],
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }
    let (s, b) = call(
        &e.app,
        "GET",
        "/api/health",
        &[("x-forwarded-for", "10.0.0.1")],
        None,
    )
    .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(b["error"]["code"], "rate_limited");
    assert_eq!(
        call(
            &e.app,
            "GET",
            "/api/health",
            &[("x-forwarded-for", "10.0.0.2")],
            None
        )
        .await
        .0,
        StatusCode::OK
    );
}
