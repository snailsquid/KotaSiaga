//! HTTP layer: routing, auth and envelopes only. No domain rules here.

mod auth;
mod city;
pub mod envelope;
#[allow(dead_code)] // some codes are used once the write endpoints land
pub mod error;
mod mock;
mod params;
mod routes;
mod user;

use axum::routing::{delete, get, post};
use axum::{Json, Router, middleware};
use serde_json::{Value, json};

use crate::AppState;
use error::{ApiError, ErrorCode};

/// All routes are served under `/api` so local paths match the public base URL.
pub fn router(state: AppState) -> Router {
    let keyed = Router::new()
        .route("/status", get(city::status))
        .route("/risk", get(city::risk))
        .route("/weather", get(city::weather))
        .route("/disasters", get(city::disasters))
        .route("/reports", get(city::reports))
        .route("/changes", get(city::changes))
        .route("/routes/safe", post(routes::safe_route))
        .route("/monitor-points", get(routes::monitor_points))
        .route("/destination-points", get(routes::destination_points));

    let keyed = if state.engine.mock_enabled() {
        let admin = Router::new()
            .route("/_mock/state", get(mock::state))
            .route("/_mock/scenario", post(mock::scenario))
            .route_layer(middleware::from_fn(auth::require_admin));
        keyed.merge(admin)
    } else {
        keyed
    };
    let keyed = keyed.route_layer(middleware::from_fn_with_state(
        state.clone(),
        auth::require_key_or_user,
    ));

    // `/reports` GET is keyed, POST is user-only: same path, two method routers merged.
    let users = Router::new()
        .route("/reports", post(user::create_report))
        .route("/reports/{id}/votes", post(user::vote))
        .route(
            "/subscriptions",
            get(user::list_subscriptions).post(user::create_subscription),
        )
        .route("/subscriptions/{id}", delete(user::delete_subscription))
        .route("/alerts", get(user::list_alerts))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_user,
        ));

    let api = Router::new()
        .route("/health", get(health))
        .merge(keyed)
        .merge(users);
    Router::new()
        .nest("/api", api)
        .fallback(|| async { ApiError::new(ErrorCode::NotFound, "Endpoint tidak ditemukan") })
        .layer(middleware::from_fn(auth::log_request))
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chrono::{Duration, Utc};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    const KEY: &str = "tourism-test-key-0001";
    const ADMIN: &str = "admin-test-key-00001";
    const SECRET: &str = "core-test-jwt-secret-0123456789abcdef";

    async fn state(mock: bool) -> AppState {
        let config = crate::config::Config::from_lookup(|k| match k {
            "MOCK_SEED" => Some("1".into()),
            "MOCK_MODE" => Some(mock.to_string()),
            "API_KEYS" => Some(format!("tourism:{KEY}")),
            "ADMIN_API_KEY" => Some(ADMIN.into()),
            "JWT_SECRET" => Some(SECRET.into()),
            // Never reach the internet from tests.
            "OSRM_URL" => Some("http://127.0.0.1:9".into()),
            "UPSTREAM_TIMEOUT_MS" => Some("500".into()),
            _ => None,
        })
        .unwrap();
        let engine = std::sync::Arc::new(crate::engine::Engine::new(&config, Utc::now()));
        let db = crate::db::open(":memory:").await.unwrap();
        engine.attach_store(
            crate::store::Loaded::default(),
            crate::store::spawn_writer(db.clone()),
            Utc::now(),
        );
        let (up, partners, routes) = crate::adapters_for(&config);
        partners.refresh_tourism(&up, Utc::now()).await;
        partners.refresh_health(&up, Utc::now()).await;
        AppState {
            config: std::sync::Arc::new(config),
            db,
            engine,
            partners,
            routes,
        }
    }

    fn token(user: &str) -> String {
        let now = Utc::now().timestamp();
        auth_token::sign(
            &auth_token::Claims {
                sub: user.into(),
                name: "Uji".into(),
                iss: auth_token::ISSUER.into(),
                aud: auth_token::AUDIENCE.into(),
                iat: now,
                exp: now + 600,
            },
            SECRET.as_bytes(),
        )
    }

    async fn call(
        st: &AppState,
        method: &str,
        uri: &str,
        body: Option<Value>,
        bearer: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut b = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(t) = bearer {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        let body = body.map_or_else(Body::empty, |v| Body::from(v.to_string()));
        let res = router(st.clone())
            .oneshot(b.body(body).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// Lets the write-behind task finish before reading the database.
    async fn flush() {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    async fn send(st: &AppState, req: Request<Body>) -> (StatusCode, Value) {
        let res = router(st.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    async fn get_with(st: &AppState, uri: &str, key: Option<&str>) -> (StatusCode, Value) {
        let mut b = Request::get(uri);
        if let Some(k) = key {
            b = b.header("x-api-key", k);
        }
        send(st, b.body(Body::empty()).unwrap()).await
    }

    async fn get(st: &AppState, uri: &str) -> (StatusCode, Value) {
        get_with(st, uri, Some(KEY)).await
    }

    #[tokio::test]
    async fn health_is_public() {
        let st = state(false).await;
        assert_eq!(
            get_with(&st, "/api/health", None).await,
            (StatusCode::OK, json!({"status":"ok"}))
        );
    }

    #[tokio::test]
    async fn unknown_path_uses_error_envelope() {
        let (status, body) = get(&state(false).await, "/api/nope").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "not_found");
        assert!(body["error"]["request_id"].is_string());
    }

    #[tokio::test]
    async fn data_endpoints_need_a_valid_key() {
        let st = state(true).await;
        for key in [None, Some("wrong-key")] {
            let (status, body) = get_with(&st, "/api/risk?lat=-6.9&lon=107.6", key).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
            assert_eq!(body["error"]["code"], "invalid_api_key");
        }
    }

    #[tokio::test]
    async fn risk_by_point_and_bbox() {
        let st = state(true).await;
        let (status, body) = get(&st, "/api/risk?lat=-6.89&lon=107.61").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["data"]["cell_id"].as_str().unwrap().starts_with("c_"));
        assert_eq!(body["data"]["factors"].as_array().unwrap().len(), 3);
        assert_eq!(body["meta"]["source"], "mock");

        let (status, body) = get(&st, "/api/risk?bbox=107.60,-6.91,107.62,-6.89").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["data"].as_array().unwrap().len() >= 4);

        let (status, body) = get(&st, "/api/risk?bbox=100,-10,110,0").await;
        assert_eq!(
            (status, body["error"]["code"].as_str()),
            (StatusCode::UNPROCESSABLE_ENTITY, Some("validation_error"))
        );
        let (status, _) = get(&st, "/api/risk?lat=-6.2&lon=106.8").await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Jakarta is outside the service area"
        );
        let (status, _) = get(&st, "/api/risk").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn weather_status_disasters_reports() {
        let st = state(true).await;
        let (status, body) = get(&st, "/api/weather?lat=-6.9&lon=107.6").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["forecast_6h"].as_array().unwrap().len(), 6);

        let (status, body) = get(&st, "/api/status?lat=-6.9&lon=107.6&radius_m=2000").await;
        assert_eq!(status, StatusCode::OK);
        for k in [
            "cell_id",
            "risk_level",
            "weather",
            "active_reports",
            "disaster_signals",
        ] {
            assert!(body["data"].get(k).is_some(), "missing {k}");
        }

        let t0 = Utc::now();
        for i in 1..=30 {
            st.engine.tick(t0 + Duration::seconds(5 * i));
        }
        let (status, body) = get(&st, "/api/reports?limit=5").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["data"].as_array().unwrap().len() <= 5);
        let (status, _) = get(&st, "/api/reports?category=tsunami").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, _) = get(&st, "/api/disasters?bbox=107.54,-6.98,107.72,-6.84").await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn weather_is_unavailable_without_a_source() {
        let (status, body) = get(&state(false).await, "/api/weather?lat=-6.9&lon=107.6").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"]["code"], "upstream_unavailable");
    }

    #[tokio::test]
    async fn changes_feed_advances_and_resets() {
        let st = state(true).await;
        let (_, first) = get(&st, "/api/changes?since=0").await;
        let seq0 = first["meta"]["seq"].as_u64().unwrap();
        let t0 = Utc::now();
        st.engine.tick(t0 + Duration::seconds(5));
        st.engine.tick(t0 + Duration::seconds(10));
        let (status, body) = get(&st, &format!("/api/changes?since={seq0}")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body["meta"]["seq"].as_u64().unwrap() > seq0,
            "seq must advance in mock mode"
        );
        let items = body["data"].as_array().unwrap();
        assert!(!items.is_empty());
        assert!(items.iter().all(|c| c["seq"].as_u64().unwrap() > seq0));
        assert!(body["meta"].get("reset").is_none());

        let (_, ahead) = get(&st, "/api/changes?since=999999").await;
        assert_eq!(ahead["meta"]["reset"], true);
        let (status, _) = get(&st, "/api/changes").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn mock_routes_need_admin_and_mock_mode() {
        let st = state(true).await;
        let (status, _) = get(&st, "/api/_mock/state").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "group key is not admin");
        let (status, body) = get_with(&st, "/api/_mock/state", Some(ADMIN)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["seed"], 1);

        let req = Request::post("/api/_mock/scenario")
            .header("x-api-key", ADMIN)
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"flood_verified","lat":-6.9,"lon":107.6}"#,
            ))
            .unwrap();
        let (status, body) = send(&st, req).await;
        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(body["data"]["active_scenario"], "flood_verified");

        let bad = Request::post("/api/_mock/scenario")
            .header("x-api-key", ADMIN)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name":"meteor"}"#))
            .unwrap();
        assert_eq!(send(&st, bad).await.0, StatusCode::UNPROCESSABLE_ENTITY);

        let live = state(false).await;
        let (status, _) = get_with(&live, "/api/_mock/state", Some(ADMIN)).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "mock routes do not exist outside mock mode"
        );
    }

    #[tokio::test]
    async fn risk_bbox_is_clipped_to_the_service_area() {
        let st = state(true).await;
        // Sticks out north and west of Bandung Raya; same cells as the clipped box.
        let (status, partly) = get(&st, "/api/risk?bbox=107.40,-6.80,107.47,-6.70").await;
        assert_eq!(status, StatusCode::OK);
        let (_, clipped) = get(&st, "/api/risk?bbox=107.45,-6.80,107.47,-6.75").await;
        let partly = partly["data"].as_array().unwrap();
        assert!(!partly.is_empty());
        assert_eq!(partly, clipped["data"].as_array().unwrap());

        let (status, body) = get(&st, "/api/risk?bbox=106.7,-6.3,106.9,-6.1").await;
        assert_eq!(
            status,
            StatusCode::OK,
            "Jakarta box is outside, not an error"
        );
        assert_eq!(body["data"], json!([]));
    }

    #[tokio::test]
    async fn status_is_unavailable_without_a_source() {
        // Documents current behaviour: no Open-Meteo adapter yet, so live mode has no weather.
        let (status, body) = get(
            &state(false).await,
            "/api/status?lat=-6.9&lon=107.6&radius_m=2000",
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"]["code"], "upstream_unavailable");
    }

    async fn scenario(st: &AppState, name: &str) -> StatusCode {
        let req = Request::post("/api/_mock/scenario")
            .header("x-api-key", ADMIN)
            .header("content-type", "application/json")
            .body(Body::from(format!(r#"{{"name":"{name}"}}"#)))
            .unwrap();
        send(st, req).await.0
    }

    async fn stale(st: &AppState) -> Value {
        get(st, "/api/risk?lat=-6.9&lon=107.6").await.1["meta"]["stale"].clone()
    }

    #[tokio::test]
    async fn upstream_down_marks_stale_until_recover_or_reset() {
        let st = state(true).await;
        for clear in ["recover", "reset"] {
            assert_eq!(scenario(&st, "upstream_down").await, StatusCode::ACCEPTED);
            assert_eq!(stale(&st).await, true, "upstream_down must set meta.stale");
            let (_, body) = get_with(&st, "/api/_mock/state", Some(ADMIN)).await;
            assert_eq!(body["data"]["upstreams_down"], json!(["open_meteo"]));
            assert_eq!(scenario(&st, clear).await, StatusCode::ACCEPTED);
            assert_eq!(stale(&st).await, false, "{clear} must clear meta.stale");
        }
    }

    #[tokio::test]
    async fn mock_state_is_read_only() {
        let st = state(true).await;
        let t0 = Utc::now();
        for i in 1..=5 {
            st.engine.tick(t0 + Duration::seconds(5 * i));
        }
        let before = st.engine.snapshot();
        for _ in 0..5 {
            let (status, body) = get_with(&st, "/api/_mock/state", Some(ADMIN)).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["meta"]["seq"].as_u64(), Some(before.seq));
            assert_eq!(body["data"]["tick"], 5);
        }
        assert!(
            std::sync::Arc::ptr_eq(&before, &st.engine.snapshot()),
            "GET /_mock/state must not publish a new snapshot"
        );
    }

    #[tokio::test]
    async fn user_endpoints_need_a_valid_token() {
        let st = state(true).await;
        let body = json!({"category": "flood", "lat": -6.9, "lon": 107.6});
        for bearer in [None, Some("garbage")] {
            let (s, b) = call(&st, "POST", "/api/reports", Some(body.clone()), bearer).await;
            assert_eq!(
                (s, b["error"]["code"].as_str()),
                (StatusCode::UNAUTHORIZED, Some("invalid_token"))
            );
        }
        // A group key is not a user.
        let req = Request::post("/api/reports")
            .header("x-api-key", KEY)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        assert_eq!(send(&st, req).await.0, StatusCode::UNAUTHORIZED);
        let (s, _) = call(&st, "GET", "/api/alerts", None, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn read_endpoints_accept_a_user_token_too() {
        let st = state(true).await;
        let t = token("usr_reader");
        let (s, b) = call(&st, "GET", "/api/risk?lat=-6.9&lon=107.6", None, Some(&t)).await;
        assert_eq!(s, StatusCode::OK);
        assert!(b["data"]["cell_id"].is_string());
    }

    #[tokio::test]
    async fn create_report_vote_and_persist() {
        let st = state(false).await;
        let alice = token("usr_alice");
        let (s, b) = call(
            &st,
            "POST",
            "/api/reports",
            Some(json!({"category": "flood", "lat": -6.9, "lon": 107.6, "description": "Air setinggi lutut\u{0007}", "severity": 3})),
            Some(&alice),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        assert_eq!(b["data"]["status"], "waiting");
        assert_eq!(b["data"]["description"], "Air setinggi lutut");
        let id = b["data"]["id"].as_str().unwrap().to_string();

        let (s, b) = call(
            &st,
            "POST",
            "/api/reports",
            Some(json!({"category": "flood", "lat": -6.2, "lon": 106.8})),
            Some(&alice),
        )
        .await;
        assert_eq!(
            (s, b["error"]["code"].as_str()),
            (StatusCode::UNPROCESSABLE_ENTITY, Some("validation_error"))
        );
        let (s, _) = call(
            &st,
            "POST",
            "/api/reports",
            Some(json!({"category": "tsunami", "lat": -6.9, "lon": 107.6})),
            Some(&alice),
        )
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

        let uri = format!("/api/reports/{id}/votes");
        let (s, b) = call(
            &st,
            "POST",
            &uri,
            Some(json!({"vote": "confirm"})),
            Some(&alice),
        )
        .await;
        assert_eq!(
            (s, b["error"]["code"].as_str()),
            (StatusCode::FORBIDDEN, Some("forbidden")),
            "own report"
        );
        let (s, _) = call(
            &st,
            "POST",
            &uri,
            Some(json!({"vote": "maybe"})),
            Some(&token("usr_bob")),
        )
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

        // Six fresh users at the default reputation 0.5 reach the 3.0 threshold.
        for i in 0..6 {
            let (s, b) = call(
                &st,
                "POST",
                &uri,
                Some(json!({"vote": "confirm"})),
                Some(&token(&format!("usr_v{i}"))),
            )
            .await;
            assert_eq!(s, StatusCode::OK, "{b}");
            let want = if i == 5 { "verified" } else { "waiting" };
            assert_eq!(b["data"]["status"], want);
            assert_eq!(b["data"]["confirm_count"], i + 1);
        }
        let (s, b) = call(
            &st,
            "POST",
            &uri,
            Some(json!({"vote": "confirm"})),
            Some(&token("usr_v0")),
        )
        .await;
        assert_eq!(
            (s, b["error"]["code"].as_str()),
            (StatusCode::CONFLICT, Some("conflict"))
        );
        let (s, _) = call(
            &st,
            "POST",
            "/api/reports/rpt_999/votes",
            Some(json!({"vote": "confirm"})),
            Some(&token("usr_v0")),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);

        flush().await;
        let (status, votes): (String, i64) = sqlx::query_as(
            "SELECT r.status, (SELECT COUNT(*) FROM verification_votes WHERE report_id = r.id) FROM reports_reports r WHERE r.id = ?",
        )
        .bind(&id)
        .fetch_one(&st.db)
        .await
        .unwrap();
        assert_eq!((status.as_str(), votes), ("verified", 6));
        let rep: (f64,) = sqlx::query_as(
            "SELECT reputation FROM verification_reputation WHERE user_id = 'usr_alice'",
        )
        .fetch_one(&st.db)
        .await
        .unwrap();
        assert!(
            rep.0 > crate::rules::REPUTATION_DEFAULT,
            "reporter gains reputation"
        );

        // A restart rebuilds the same report from SQLite.
        let loaded = crate::store::load(&st.db, Utc::now()).await.unwrap();
        assert_eq!(loaded.live.len(), 1);
        assert_eq!(loaded.live[0].id, id);
        assert_eq!(
            loaded.max_report_seq,
            id.trim_start_matches("rpt_").parse::<u64>().unwrap()
        );
    }

    #[tokio::test]
    async fn report_rate_limit_per_user() {
        let st = state(false).await;
        let t = token("usr_spam");
        for _ in 0..crate::rules::MAX_REPORTS_PER_USER_PER_HOUR {
            let (s, _) = call(
                &st,
                "POST",
                "/api/reports",
                Some(json!({"category": "accident", "lat": -6.9, "lon": 107.6})),
                Some(&t),
            )
            .await;
            assert_eq!(s, StatusCode::CREATED);
        }
        let (s, b) = call(
            &st,
            "POST",
            "/api/reports",
            Some(json!({"category": "accident", "lat": -6.9, "lon": 107.6})),
            Some(&t),
        )
        .await;
        assert_eq!(
            (s, b["error"]["code"].as_str()),
            (StatusCode::TOO_MANY_REQUESTS, Some("rate_limited"))
        );
    }

    #[tokio::test]
    async fn subscriptions_and_alerts() {
        let st = state(true).await;
        let t = token("usr_sub");
        let (s, b) = call(
            &st,
            "POST",
            "/api/subscriptions",
            Some(json!({"name": "Rumah", "lat": -6.9, "lon": 107.6, "radius_m": 800})),
            Some(&t),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        let sub_id = b["data"]["id"].as_str().unwrap().to_string();
        assert!(b["data"]["risk_level"].is_string());
        let (s, _) = call(
            &st,
            "POST",
            "/api/subscriptions",
            Some(json!({"name": "", "lat": -6.9, "lon": 107.6})),
            Some(&t),
        )
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
        let (s, _) = call(
            &st,
            "POST",
            "/api/subscriptions",
            Some(json!({"name": "Jauh", "lat": -6.9, "lon": 107.6, "radius_m": 9000})),
            Some(&t),
        )
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

        let (_, b) = call(&st, "GET", "/api/subscriptions", None, Some(&t)).await;
        assert_eq!(b["data"].as_array().unwrap().len(), 1);
        let (_, other) = call(
            &st,
            "GET",
            "/api/subscriptions",
            None,
            Some(&token("usr_other")),
        )
        .await;
        assert_eq!(other["data"], json!([]), "users only see their own");

        // Two verified severe floods at the point push the area to danger -> one alert.
        for _ in 0..2 {
            let req = Request::post("/api/_mock/scenario")
                .header("x-api-key", ADMIN)
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"name":"flood_verified","lat":-6.9,"lon":107.6}"#,
                ))
                .unwrap();
            assert_eq!(send(&st, req).await.0, StatusCode::ACCEPTED);
        }
        flush().await;
        let (s, b) = call(&st, "GET", "/api/alerts", None, Some(&t)).await;
        assert_eq!(s, StatusCode::OK);
        let alerts = b["data"].as_array().unwrap();
        assert!(!alerts.is_empty(), "risk rose inside the subscribed area");
        assert_eq!(alerts[0]["subscription_id"], sub_id.as_str());
        assert_eq!(alerts[0]["risk_level"], "danger");
        assert!(alerts[0]["message"].as_str().unwrap().starts_with("Rumah"));
        let (_, none) = call(&st, "GET", "/api/alerts", None, Some(&token("usr_other"))).await;
        assert_eq!(none["data"], json!([]));

        let (s, _) = call(
            &st,
            "DELETE",
            &format!("/api/subscriptions/{sub_id}"),
            None,
            Some(&token("usr_other")),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "cannot delete someone else's");
        let (s, _) = call(
            &st,
            "DELETE",
            &format!("/api/subscriptions/{sub_id}"),
            None,
            Some(&t),
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (_, b) = call(&st, "GET", "/api/alerts", None, Some(&t)).await;
        assert_eq!(b["data"], json!([]), "alerts go with their subscription");
    }

    #[tokio::test]
    async fn safe_route_in_mock_mode() {
        let st = state(true).await;
        let req = Request::post("/api/routes/safe")
            .header("x-api-key", KEY)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"origin":{"lat":-6.90,"lon":107.60},"destination":{"lat":-6.91,"lon":107.62},"alternatives":3}"#))
            .unwrap();
        let (s, b) = send(&st, req).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        let routes = b["data"]["routes"].as_array().unwrap();
        assert_eq!(routes.len(), 3);
        assert_eq!(b["data"]["recommended_id"], routes[0]["id"]);
        assert_eq!(routes[0]["geometry"]["type"], "LineString");
        assert!(!routes[0]["reasons"].as_array().unwrap().is_empty());

        // By destination point id (a Health donation event).
        let (_, pts) = get(&st, "/api/destination-points").await;
        let dest = pts["data"][0]["id"].as_str().unwrap().to_string();
        let req = Request::post("/api/routes/safe")
            .header("x-api-key", KEY)
            .header("content-type", "application/json")
            .body(Body::from(format!(
                r#"{{"origin":{{"lat":-6.90,"lon":107.60}},"destination_id":"{dest}"}}"#
            )))
            .unwrap();
        assert_eq!(send(&st, req).await.0, StatusCode::OK);

        for bad in [
            r#"{"origin":{"lat":-6.2,"lon":106.8},"destination":{"lat":-6.91,"lon":107.62}}"#,
            r#"{"origin":{"lat":-6.90,"lon":107.60}}"#,
            r#"{"origin":{"lat":-6.90,"lon":107.60},"destination":{"lat":-6.91,"lon":107.62},"alternatives":7}"#,
        ] {
            let req = Request::post("/api/routes/safe")
                .header("x-api-key", KEY)
                .header("content-type", "application/json")
                .body(Body::from(bad))
                .unwrap();
            assert_eq!(
                send(&st, req).await.0,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{bad}"
            );
        }
    }

    #[tokio::test]
    async fn live_route_without_osrm_is_a_clear_503() {
        let st = state(false).await; // OSRM_URL points at a closed port
        let req = Request::post("/api/routes/safe")
            .header("x-api-key", KEY)
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"origin":{"lat":-6.90,"lon":107.60},"destination":{"lat":-6.91,"lon":107.62}}"#,
            ))
            .unwrap();
        let res = router(st.clone()).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(res.headers().contains_key("retry-after"));
    }

    #[tokio::test]
    async fn partner_points_carry_risk() {
        let st = state(true).await;
        let (s, b) = get(&st, "/api/monitor-points").await;
        assert_eq!(s, StatusCode::OK);
        let pts = b["data"].as_array().unwrap();
        assert_eq!(pts.len(), 11);
        assert!(pts[0]["risk_level"].is_string());
        assert!(pts[0]["cell_id"].as_str().unwrap().starts_with("c_"));
        assert_eq!(b["meta"]["source"], "mock");
        let (s, b) = get(&st, "/api/destination-points?risk_level=safe").await;
        assert_eq!(s, StatusCode::OK);
        assert!(
            b["data"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["risk_level"] == "safe")
        );
        let (s, _) = get(&st, "/api/monitor-points?risk_level=meh").await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    }

    async fn stored_report(st: &AppState, id: &str) -> Option<(String, i64)> {
        sqlx::query_as("SELECT status, confirm_count FROM reports_reports WHERE id = ?")
            .bind(id)
            .fetch_optional(&st.db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn writes_are_committed_before_the_reply() {
        let st = state(false).await;
        let (s, b) = call(
            &st,
            "POST",
            "/api/reports",
            Some(json!({"category": "road_damage", "lat": -6.9, "lon": 107.6})),
            Some(&token("usr_w1")),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
        let id = b["data"]["id"].as_str().unwrap().to_string();
        // No sleep: the 201 itself promises the row is in SQLite.
        assert_eq!(stored_report(&st, &id).await, Some(("waiting".into(), 0)));
        let (s, _) = call(
            &st,
            "POST",
            &format!("/api/reports/{id}/votes"),
            Some(json!({"vote": "confirm"})),
            Some(&token("usr_w2")),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(stored_report(&st, &id).await, Some(("waiting".into(), 1)));
    }

    /// What main() does on SIGTERM: stop changing state, flush, exit. Nothing queued is lost.
    #[tokio::test]
    async fn shutdown_flush_keeps_queued_writes() {
        let st = state(false).await;
        let now = Utc::now();
        let ids: Vec<String> = (0..50)
            .map(|i| {
                st.engine.update(now, |w| {
                    w.create_report(
                        crate::reports::NewReport {
                            category: crate::reports::Category::Accident,
                            lat: -6.9,
                            lon: 107.6,
                            description: Some(format!("antre {i}")),
                            severity: 1,
                            reporter: "usr_bulk".into(),
                            origin: crate::reports::Origin::User,
                        },
                        now,
                    )
                    .unwrap()
                    .id
                })
            })
            .collect();
        st.engine.flush().await.unwrap();
        let loaded = crate::store::load(&st.db, Utc::now()).await.unwrap();
        for id in &ids {
            assert!(loaded.live.iter().any(|r| &r.id == id), "{id} lost");
        }
    }

    #[tokio::test]
    async fn mock_reset_keeps_user_reports() {
        let st = state(true).await;
        let (s, b) = call(
            &st,
            "POST",
            "/api/reports",
            Some(json!({"category": "flood", "lat": -6.9, "lon": 107.6})),
            Some(&token("usr_keep")),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
        let id = b["data"]["id"].as_str().unwrap().to_string();
        let (s, _) = call(
            &st,
            "POST",
            &format!("/api/reports/{id}/votes"),
            Some(json!({"vote": "confirm"})),
            Some(&token("usr_voter")),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        let t0 = Utc::now();
        for i in 1..=10 {
            st.engine.tick(t0 + Duration::seconds(5 * i));
        }
        // Mock users may have voted on it meanwhile; whatever is stored must survive the reset.
        st.engine.flush().await.unwrap();
        let before = stored_report(&st, &id).await;
        assert!(before.is_some());
        assert_eq!(scenario(&st, "reset").await, StatusCode::ACCEPTED);
        st.engine.flush().await.unwrap();

        let s2 = st.engine.snapshot();
        assert!(
            s2.reports.iter().any(|r| r.id == id),
            "user report survives reset in memory"
        );
        assert!(
            s2.reports
                .iter()
                .all(|r| r.origin == crate::reports::Origin::User),
            "mock reports are gone"
        );
        assert_eq!(
            stored_report(&st, &id).await,
            before,
            "and in SQLite, unchanged"
        );
        let votes: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM verification_votes WHERE report_id = ?")
                .bind(&id)
                .fetch_one(&st.db)
                .await
                .unwrap();
        assert!(votes.0 >= 1, "its votes stay stored");
        // Its ballot is kept too: the same user still cannot vote twice.
        let (s, _) = call(
            &st,
            "POST",
            &format!("/api/reports/{id}/votes"),
            Some(json!({"vote": "confirm"})),
            Some(&token("usr_voter")),
        )
        .await;
        assert_eq!(s, StatusCode::CONFLICT);
    }

    /// Regression for the tick vs /_mock lock-order deadlock: ticks on one OS thread,
    /// /_mock/state and /_mock/scenario from several tasks at the same time, for a fixed
    /// wall time. Fails (does not hang) if they ever block each other.
    #[test]
    fn ticks_and_mock_calls_do_not_deadlock() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        use std::time::{Duration as StdDuration, Instant};
        const RUN_FOR: StdDuration = StdDuration::from_secs(2);
        const CALLERS: usize = 4;

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(CALLERS)
                .enable_all()
                .build()
                .unwrap();
            let counts = rt.block_on(async {
                let st = state(true).await;
                let stop = Arc::new(AtomicBool::new(false));
                let ticks = Arc::new(AtomicU64::new(0));
                let ticker = {
                    let (engine, stop, ticks) = (st.engine.clone(), stop.clone(), ticks.clone());
                    std::thread::spawn(move || {
                        let t0 = Utc::now();
                        while !stop.load(Ordering::Relaxed) {
                            let n = ticks.fetch_add(1, Ordering::Relaxed) as i64 + 1;
                            engine.tick(t0 + Duration::seconds(5 * n));
                            std::thread::yield_now();
                        }
                    })
                };
                let calls = Arc::new(AtomicU64::new(0));
                let started = Instant::now();
                let tasks: Vec<_> = (0..CALLERS)
                    .map(|k| {
                        let (st, calls) = (st.clone(), calls.clone());
                        tokio::spawn(async move {
                            let mut i = k;
                            while started.elapsed() < RUN_FOR {
                                let (status, _) =
                                    get_with(&st, "/api/_mock/state", Some(ADMIN)).await;
                                assert_eq!(status, StatusCode::OK);
                                let name = crate::mock::SCENARIOS[i % crate::mock::SCENARIOS.len()];
                                assert_eq!(scenario(&st, name).await, StatusCode::ACCEPTED);
                                calls.fetch_add(2, Ordering::Relaxed);
                                i += 1;
                            }
                        })
                    })
                    .collect();
                for t in tasks {
                    t.await.unwrap();
                }
                stop.store(true, Ordering::Relaxed);
                ticker.join().unwrap();
                (ticks.load(Ordering::Relaxed), calls.load(Ordering::Relaxed))
            });
            let _ = tx.send(counts);
        });
        let (ticks, calls) = rx
            .recv_timeout(RUN_FOR + StdDuration::from_secs(30))
            .expect("ticks and /_mock calls did not finish: deadlock between tick and /_mock?");
        assert!(ticks > 0 && calls > 0, "ticks {ticks}, calls {calls}");
    }
}
