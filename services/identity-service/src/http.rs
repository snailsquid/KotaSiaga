//! /api/auth/* handlers. Envelopes and error codes follow AGENTS.md section 3.

use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Request, State, rejection::JsonRejection};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tokio::sync::Semaphore;

use crate::config::Config;
use crate::password;
use crate::store::{self, CreateError, User};

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    db: SqlitePool,
    /// Caps concurrent Argon2 work so a login burst cannot exhaust the 64 MB limit.
    hash_slots: Arc<Semaphore>,
    dummy_hash: Arc<String>,
}

impl AppState {
    pub fn new(config: Config, db: SqlitePool) -> Self {
        Self {
            config: Arc::new(config),
            db,
            hash_slots: Arc::new(Semaphore::new(2)),
            dummy_hash: Arc::new(password::dummy_hash()),
        }
    }
}

pub fn router(state: AppState) -> Router {
    let auth = Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/me", get(me));
    Router::new()
        .nest(
            "/api",
            Router::new()
                .route("/health", get(|| async { Json(json!({"status": "ok"})) }))
                .nest("/auth", auth),
        )
        .fallback(|| async {
            err(
                StatusCode::NOT_FOUND,
                "not_found",
                "Endpoint tidak ditemukan",
            )
        })
        .layer(middleware::from_fn(log_request))
        .with_state(state)
}

pub struct ApiError(StatusCode, &'static str, String);

fn err(status: StatusCode, code: &'static str, msg: impl Into<String>) -> ApiError {
    ApiError(status, code, msg.into())
}

fn invalid(msg: impl Into<String>) -> ApiError {
    err(StatusCode::UNPROCESSABLE_ENTITY, "validation_error", msg)
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    tracing::error!(error = %e, "database error");
    err(
        StatusCode::SERVICE_UNAVAILABLE,
        "upstream_unavailable",
        "Layanan akun sedang bermasalah",
    )
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({"error": {"code": self.1, "message": self.2, "request_id": uuid::Uuid::new_v4().to_string()}});
        (self.0, Json(body)).into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;

fn secret(st: &AppState) -> ApiResult<&[u8]> {
    st.config.jwt_secret.as_deref().ok_or_else(|| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "upstream_unavailable",
            "Layanan login belum dikonfigurasi",
        )
    })
}

fn body<T>(b: Result<Json<T>, JsonRejection>) -> ApiResult<T> {
    b.map(|Json(v)| v)
        .map_err(|e| invalid(format!("Body tidak valid: {}", e.body_text())))
}

pub fn normalize_email(raw: &str) -> ApiResult<String> {
    let e = raw.trim().to_lowercase();
    let ok = e.len() <= 254
        && e.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !domain.contains('@')
        })
        && !e.chars().any(char::is_whitespace);
    if ok {
        Ok(e)
    } else {
        Err(invalid("email tidak valid"))
    }
}

#[derive(Deserialize)]
struct RegisterRequest {
    email: String,
    password: String,
    display_name: String,
}

#[derive(Serialize)]
struct UserView {
    id: String,
    email: String,
    display_name: String,
}

impl From<&User> for UserView {
    fn from(u: &User) -> Self {
        Self {
            id: u.id.clone(),
            email: u.email.clone(),
            display_name: u.display_name.clone(),
        }
    }
}

async fn hash_limited(st: &AppState, pw: String) -> String {
    let _permit = st.hash_slots.acquire().await.expect("semaphore");
    tokio::task::spawn_blocking(move || password::hash(&pw))
        .await
        .expect("hash task")
}

async fn verify_limited(st: &AppState, pw: String, phc: String) -> bool {
    let _permit = st.hash_slots.acquire().await.expect("semaphore");
    tokio::task::spawn_blocking(move || password::verify(&pw, &phc))
        .await
        .unwrap_or(false)
}

async fn register(
    State(st): State<AppState>,
    b: Result<Json<RegisterRequest>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    secret(&st)?;
    let req = body(b)?;
    let email = normalize_email(&req.email)?;
    let name = req.display_name.trim().to_string();
    if !(1..=60).contains(&name.chars().count()) {
        return Err(invalid("display_name harus 1 sampai 60 karakter"));
    }
    if !(8..=128).contains(&req.password.chars().count()) {
        return Err(invalid("password harus 8 sampai 128 karakter"));
    }
    let hash = hash_limited(&st, req.password).await;
    match store::create_user(&st.db, &email, &name, &hash, Utc::now()).await {
        Ok(u) => {
            tracing::info!(user_id = u.id, "user registered");
            Ok((
                StatusCode::CREATED,
                Json(json!({"data": UserView::from(&u)})),
            ))
        }
        Err(CreateError::EmailTaken) => Err(err(
            StatusCode::CONFLICT,
            "conflict",
            "Email sudah terdaftar",
        )),
        Err(CreateError::Db(e)) => Err(internal(e)),
    }
}

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

async fn tokens(st: &AppState, user: &User) -> ApiResult<Json<Value>> {
    let secret = secret(st)?;
    let now = Utc::now();
    let claims = auth_token::Claims {
        sub: user.id.clone(),
        name: user.display_name.clone(),
        iss: auth_token::ISSUER.into(),
        aud: auth_token::AUDIENCE.into(),
        iat: now.timestamp(),
        exp: now.timestamp() + st.config.access_ttl_s,
    };
    let access = auth_token::sign(&claims, secret);
    let refresh = store::issue_refresh(&st.db, &user.id, now, st.config.refresh_ttl_s)
        .await
        .map_err(internal)?;
    Ok(Json(json!({"data": {
        "access_token": access,
        "token_type": "bearer",
        "expires_in": st.config.access_ttl_s,
        "refresh_token": refresh,
        "refresh_expires_in": st.config.refresh_ttl_s,
        "user": UserView::from(user),
    }})))
}

fn bad_credentials() -> ApiError {
    err(
        StatusCode::UNAUTHORIZED,
        "invalid_credentials",
        "Email atau password salah",
    )
}

async fn login(
    State(st): State<AppState>,
    b: Result<Json<LoginRequest>, JsonRejection>,
) -> ApiResult<Json<Value>> {
    secret(&st)?;
    let req = body(b)?;
    let email = normalize_email(&req.email).map_err(|_| bad_credentials())?;
    let user = store::user_by_email(&st.db, &email)
        .await
        .map_err(internal)?;
    // Same Argon2 cost whether or not the email exists, so timing does not reveal accounts.
    let phc = user
        .as_ref()
        .map_or_else(|| st.dummy_hash.to_string(), |u| u.password_hash.clone());
    let ok = verify_limited(&st, req.password, phc).await;
    match user {
        Some(u) if ok => {
            tracing::info!(user_id = u.id, "login ok");
            tokens(&st, &u).await
        }
        _ => {
            tracing::info!("login failed");
            Err(bad_credentials())
        }
    }
}

#[derive(Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

fn bad_refresh() -> ApiError {
    err(
        StatusCode::UNAUTHORIZED,
        "invalid_token",
        "Refresh token tidak valid atau kedaluwarsa",
    )
}

async fn refresh(
    State(st): State<AppState>,
    b: Result<Json<RefreshRequest>, JsonRejection>,
) -> ApiResult<Json<Value>> {
    secret(&st)?;
    let req = body(b)?;
    let user_id = store::consume_refresh(&st.db, &req.refresh_token, Utc::now())
        .await
        .map_err(internal)?
        .ok_or_else(bad_refresh)?;
    let user = store::user_by_id(&st.db, &user_id)
        .await
        .map_err(internal)?
        .ok_or_else(bad_refresh)?;
    tokens(&st, &user).await
}

async fn logout(
    State(st): State<AppState>,
    b: Result<Json<RefreshRequest>, JsonRejection>,
) -> ApiResult<StatusCode> {
    let req = body(b)?;
    store::consume_refresh(&st.db, &req.refresh_token, Utc::now())
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn me(State(st): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let secret = secret(&st)?;
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
        .ok_or_else(|| {
            err(
                StatusCode::UNAUTHORIZED,
                "invalid_token",
                "Header Authorization: Bearer <token> wajib",
            )
        })?;
    let claims = auth_token::verify(token.trim(), secret, Utc::now().timestamp()).map_err(|e| {
        err(
            StatusCode::UNAUTHORIZED,
            "invalid_token",
            format!("Token tidak valid: {e}"),
        )
    })?;
    let user = store::user_by_id(&st.db, &claims.sub)
        .await
        .map_err(internal)?
        .ok_or_else(|| {
            err(
                StatusCode::UNAUTHORIZED,
                "invalid_token",
                "Akun tidak ditemukan",
            )
        })?;
    Ok(Json(json!({"data": UserView::from(&user)})))
}

/// One access-log line per request. Never logs bodies (they carry passwords and tokens).
async fn log_request(req: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let res = next.run(req).await;
    tracing::info!(target: "access", %method, path, status = res.status().as_u16(), ms = started.elapsed().as_millis() as u64);
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    const SECRET: &str = "identity-test-secret-0123456789abcdef";

    /// Test passwords are generated per run: no credential-looking literals in the repo.
    fn random_pw() -> String {
        format!("pw-{}", uuid::Uuid::new_v4().simple())
    }

    static TEST_PW: std::sync::LazyLock<String> = std::sync::LazyLock::new(random_pw);

    async fn state(with_secret: bool) -> AppState {
        let config = Config::from_lookup(|k| match k {
            "JWT_SECRET" if with_secret => Some(SECRET.into()),
            "TEST_ACCOUNT_PASSWORD" => Some(TEST_PW.clone()),
            _ => None,
        })
        .unwrap();
        let db = store::open(":memory:").await.unwrap();
        store::upsert_test_account(&db, config.test_account.as_ref().unwrap())
            .await
            .unwrap();
        AppState::new(config, db)
    }

    async fn call(
        st: &AppState,
        method: &str,
        uri: &str,
        body: Value,
        bearer: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut b = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(t) = bearer {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        let res = router(st.clone())
            .oneshot(b.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn health_is_ok() {
        let st = state(true).await;
        let (s, b) = call(&st, "GET", "/api/health", Value::Null, None).await;
        assert_eq!((s, b), (StatusCode::OK, json!({"status": "ok"})));
    }

    #[tokio::test]
    async fn register_login_me_refresh_logout() {
        let st = state(true).await;
        let good = random_pw();
        let reg =
            json!({"email": "Warga@Contoh.test", "password": good, "display_name": "Warga Uji"});
        let (s, b) = call(&st, "POST", "/api/auth/register", reg.clone(), None).await;
        assert_eq!(s, StatusCode::CREATED);
        assert_eq!(b["data"]["email"], "warga@contoh.test");
        assert!(b["data"]["id"].as_str().unwrap().starts_with("usr_"));
        assert!(b["data"].get("password_hash").is_none());

        let (s, b) = call(&st, "POST", "/api/auth/register", reg, None).await;
        assert_eq!(
            (s, b["error"]["code"].as_str()),
            (StatusCode::CONFLICT, Some("conflict"))
        );

        let (s, b) = call(
            &st,
            "POST",
            "/api/auth/login",
            json!({"email": "warga@contoh.test", "password": random_pw()}),
            None,
        )
        .await;
        assert_eq!(
            (s, b["error"]["code"].as_str()),
            (StatusCode::UNAUTHORIZED, Some("invalid_credentials"))
        );
        let (s, _) = call(
            &st,
            "POST",
            "/api/auth/login",
            json!({"email": "nobody@contoh.test", "password": good}),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);

        let (s, b) = call(
            &st,
            "POST",
            "/api/auth/login",
            json!({"email": "warga@contoh.test", "password": good}),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(b["data"]["token_type"], "bearer");
        let access = b["data"]["access_token"].as_str().unwrap().to_string();
        let refresh = b["data"]["refresh_token"].as_str().unwrap().to_string();
        let claims =
            auth_token::verify(&access, SECRET.as_bytes(), Utc::now().timestamp()).unwrap();
        assert_eq!(claims.name, "Warga Uji");

        let (s, b) = call(&st, "GET", "/api/auth/me", Value::Null, Some(&access)).await;
        assert_eq!(
            (s, b["data"]["display_name"].as_str()),
            (StatusCode::OK, Some("Warga Uji"))
        );
        let (s, _) = call(&st, "GET", "/api/auth/me", Value::Null, Some("garbage")).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);

        let (s, b) = call(
            &st,
            "POST",
            "/api/auth/refresh",
            json!({"refresh_token": refresh}),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        let rotated = b["data"]["refresh_token"].as_str().unwrap().to_string();
        assert_ne!(rotated, refresh);
        let (s, _) = call(
            &st,
            "POST",
            "/api/auth/refresh",
            json!({"refresh_token": refresh}),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "a refresh token works once");

        let (s, _) = call(
            &st,
            "POST",
            "/api/auth/logout",
            json!({"refresh_token": rotated}),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (s, _) = call(
            &st,
            "POST",
            "/api/auth/refresh",
            json!({"refresh_token": rotated}),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "logout revokes");
    }

    #[tokio::test]
    async fn test_account_can_log_in() {
        let st = state(true).await;
        let (s, b) = call(
            &st,
            "POST",
            "/api/auth/login",
            json!({"email": crate::config::DEFAULT_TEST_EMAIL, "password": TEST_PW.as_str()}),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(
            b["data"]["user"]["display_name"],
            crate::config::DEFAULT_TEST_NAME
        );
    }

    #[tokio::test]
    async fn validation_errors() {
        let st = state(true).await;
        let ok = random_pw();
        for bad in [
            json!({"email": "not-an-email", "password": ok, "display_name": "x"}),
            json!({"email": "a@b.test", "password": "short", "display_name": "x"}),
            json!({"email": "a@b.test", "password": ok, "display_name": "  "}),
            json!({"email": "a@b.test"}),
        ] {
            let (s, b) = call(&st, "POST", "/api/auth/register", bad.clone(), None).await;
            assert_eq!(
                (s, b["error"]["code"].as_str()),
                (StatusCode::UNPROCESSABLE_ENTITY, Some("validation_error")),
                "{bad}"
            );
        }
    }

    #[tokio::test]
    async fn without_secret_auth_is_503_not_a_crash() {
        let st = state(false).await;
        let (s, b) = call(
            &st,
            "POST",
            "/api/auth/login",
            json!({"email": "a@b.test", "password": "x"}),
            None,
        )
        .await;
        assert_eq!(
            (s, b["error"]["code"].as_str()),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Some("upstream_unavailable")
            )
        );
    }
}
