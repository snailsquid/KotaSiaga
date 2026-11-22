//! SQLite (WAL) store for users and refresh tokens. Tables are prefixed `identity_`.

use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::Row;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};

use crate::config::TestAccount;
use crate::password;

pub async fn open(path: &str) -> Result<SqlitePool, sqlx::Error> {
    let url = if path == ":memory:" {
        "sqlite::memory:".to_string()
    } else {
        format!("sqlite://{path}")
    };
    let opts = SqliteConnectOptions::from_str(&url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    // In-memory databases are per connection, so tests use a single one.
    let max = if path == ":memory:" { 1 } else { 4 };
    let pool = SqlitePoolOptions::new()
        .max_connections(max)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
    #[serde(skip)]
    pub password_hash: String,
}

fn user_from(row: &sqlx::sqlite::SqliteRow) -> User {
    User {
        id: row.get("id"),
        email: row.get("email"),
        display_name: row.get("display_name"),
        password_hash: row.get("password_hash"),
    }
}

pub fn new_user_id() -> String {
    format!("usr_{}", &uuid::Uuid::new_v4().simple().to_string()[..20])
}

#[derive(Debug)]
pub enum CreateError {
    EmailTaken,
    Db(sqlx::Error),
}

pub async fn create_user(
    db: &SqlitePool,
    email: &str,
    display_name: &str,
    password_hash: &str,
    now: DateTime<Utc>,
) -> Result<User, CreateError> {
    let id = new_user_id();
    let res = sqlx::query(
        "INSERT INTO identity_users (id, email, display_name, password_hash, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(email)
    .bind(display_name)
    .bind(password_hash)
    .bind(now.to_rfc3339())
    .execute(db)
    .await;
    match res {
        Ok(_) => Ok(User {
            id,
            email: email.into(),
            display_name: display_name.into(),
            password_hash: password_hash.into(),
        }),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(CreateError::EmailTaken),
        Err(e) => Err(CreateError::Db(e)),
    }
}

pub async fn user_by_email(db: &SqlitePool, email: &str) -> Result<Option<User>, sqlx::Error> {
    Ok(sqlx::query("SELECT * FROM identity_users WHERE email = ?")
        .bind(email)
        .fetch_optional(db)
        .await?
        .as_ref()
        .map(user_from))
}

pub async fn user_by_id(db: &SqlitePool, id: &str) -> Result<Option<User>, sqlx::Error> {
    Ok(sqlx::query("SELECT * FROM identity_users WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?
        .as_ref()
        .map(user_from))
}

/// Creates the documented test account, or resets its name and password to the env values.
pub async fn upsert_test_account(
    db: &SqlitePool,
    acc: &TestAccount,
) -> Result<String, sqlx::Error> {
    let hash = password::hash(&acc.password);
    if let Some(u) = user_by_email(db, &acc.email).await? {
        sqlx::query("UPDATE identity_users SET display_name = ?, password_hash = ?, is_test_account = 1 WHERE id = ?")
            .bind(&acc.display_name)
            .bind(&hash)
            .bind(&u.id)
            .execute(db)
            .await?;
        return Ok(u.id);
    }
    let id = new_user_id();
    sqlx::query(
        "INSERT INTO identity_users (id, email, display_name, password_hash, is_test_account, created_at) VALUES (?, ?, ?, ?, 1, ?)",
    )
    .bind(&id)
    .bind(&acc.email)
    .bind(&acc.display_name)
    .bind(&hash)
    .bind(Utc::now().to_rfc3339())
    .execute(db)
    .await?;
    Ok(id)
}

fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Issues a new opaque refresh token (256 random bits as hex) and stores its hash.
pub async fn issue_refresh(
    db: &SqlitePool,
    user_id: &str,
    now: DateTime<Utc>,
    ttl_s: i64,
) -> Result<String, sqlx::Error> {
    let token = format!(
        "rt_{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    sqlx::query("INSERT INTO identity_refresh_tokens (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)")
        .bind(token_hash(&token))
        .bind(user_id)
        .bind(now.to_rfc3339())
        .bind((now + chrono::Duration::seconds(ttl_s)).to_rfc3339())
        .execute(db)
        .await?;
    // Housekeeping: drop this user's expired or revoked tokens.
    sqlx::query("DELETE FROM identity_refresh_tokens WHERE user_id = ? AND (expires_at < ? OR revoked_at IS NOT NULL)")
        .bind(user_id)
        .bind(now.to_rfc3339())
        .execute(db)
        .await?;
    Ok(token)
}

/// Revokes a live refresh token and returns its user id. `None` if unknown, expired or revoked.
pub async fn consume_refresh(
    db: &SqlitePool,
    token: &str,
    now: DateTime<Utc>,
) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query(
        "UPDATE identity_refresh_tokens SET revoked_at = ? \
         WHERE token_hash = ? AND revoked_at IS NULL AND expires_at > ? RETURNING user_id",
    )
    .bind(now.to_rfc3339())
    .bind(token_hash(token))
    .bind(now.to_rfc3339())
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| r.get("user_id")))
}
