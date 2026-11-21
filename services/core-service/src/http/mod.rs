//! HTTP layer: routing, auth and envelopes only. No domain rules here.

mod auth;
mod city;
pub mod envelope;
#[allow(dead_code)] // some codes are used once the write endpoints land
pub mod error;
mod mock;
mod params;

use axum::routing::{get, post};
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
        .route("/changes", get(city::changes));

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
        auth::require_api_key,
    ));

    let api = Router::new().route("/health", get(health)).merge(keyed);
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

    async fn state(mock: bool) -> AppState {
        let config = crate::config::Config::from_lookup(|k| match k {
            "MOCK_SEED" => Some("1".into()),
            "MOCK_MODE" => Some(mock.to_string()),
            "API_KEYS" => Some(format!("tourism:{KEY}")),
            "ADMIN_API_KEY" => Some(ADMIN.into()),
            _ => None,
        })
        .unwrap();
        let engine = std::sync::Arc::new(crate::engine::Engine::new(&config, Utc::now()));
        AppState {
            config: std::sync::Arc::new(config),
            db: crate::db::open(":memory:").await.unwrap(),
            engine,
        }
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
