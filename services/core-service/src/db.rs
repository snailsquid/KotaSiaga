//! SQLite in WAL mode (AGENTS.md section 9). One file for core-service; each context
//! owns its own tables, prefixed with the context name (e.g. `reports_`, `risk_`).

use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};

pub async fn open(path: &str) -> Result<SqlitePool, sqlx::Error> {
    let url = if path == ":memory:" {
        "sqlite::memory:".to_string()
    } else {
        format!("sqlite://{path}")
    };
    let opts = SqliteConnectOptions::from_str(&url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        // NORMAL is durable across app crashes in WAL mode; only a power cut can lose the last commits.
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    // Small pool: reads are served from memory, the database mostly takes writes.
    // An in-memory database exists per connection, so tests use exactly one.
    let pool = SqlitePoolOptions::new()
        .max_connections(if path == ":memory:" { 1 } else { 4 })
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn opens_in_wal_mode() {
        let dir = std::env::temp_dir().join(format!("kotasiaga-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("core.db");
        let pool = super::open(path.to_str().unwrap()).await.unwrap();
        let mode: (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(mode.0.to_lowercase(), "wal");
        pool.close().await;
        let _ = std::fs::remove_dir_all(dir);
    }
}
