//! KotaSiaga identity-service (Identitas & Akses): app user accounts and JWT issuance.
//! Runs in its own container with its own SQLite file. core-service verifies the tokens
//! it issues with the same `JWT_SECRET`; group API keys (`X-API-Key`) live in core's config.

mod config;
mod http;
mod password;
mod store;

use std::net::SocketAddr;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tune_malloc();
    if std::env::args().nth(1).as_deref() == Some("backup") {
        backup().await;
        return;
    }
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        // Plain text in containers so `docker compose logs` stays greppable (call-log evidence).
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .init();

    let config = config::Config::from_env()
        .unwrap_or_else(|e| fatal(&format!("invalid configuration: {e}")));
    if config.jwt_secret.is_none() {
        tracing::error!(
            "JWT_SECRET is not set (or shorter than {} bytes): register/login return 503 until it is",
            auth_token::MIN_SECRET_LEN
        );
    }
    let db = store::open(&config.database_path).await.unwrap_or_else(|e| {
        fatal(&format!(
            "cannot open database {}: {e} (does its directory exist and is it writable by this user?)",
            config.database_path
        ))
    });
    if let Some(acc) = &config.test_account {
        match store::upsert_test_account(&db, acc).await {
            Ok(id) => tracing::info!(email = acc.email, user_id = id, "test account ready"),
            Err(e) => tracing::error!(error = %e, "could not seed the test account"),
        }
    } else {
        tracing::warn!("TEST_ACCOUNT_PASSWORD not set: no test account seeded");
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let app = http::router(http::AppState::new(config, db));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| fatal(&format!("cannot bind {addr}: {e}")));
    tracing::info!(%addr, "identity-service listening");
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        fatal(&format!("server error: {e}"));
    }
}

/// Logs a startup failure straight to stderr (not through tracing, so RUST_LOG cannot hide it)
/// and exits. The runtime image is distroless with no shell, so this line is the only clue.
fn fatal(msg: &str) -> ! {
    eprintln!("identity-service: fatal: {msg}");
    std::process::exit(1);
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}

/// `identity-service backup [dest]`: consistent online copy with `VACUUM INTO` (safe in WAL mode while the
/// service runs). Works in the distroless image: `docker compose exec identity-service /app backup`.
/// Default destination `/data/backup.db` (replaced each time); prints the path and the result
/// of `PRAGMA integrity_check` on the copy.
async fn backup() {
    let src = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "/data/identity.db".into());
    let dest = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "/data/backup.db".into());
    let _ = std::fs::remove_file(&dest);
    let db = store::open(&src)
        .await
        .unwrap_or_else(|e| fatal_backup(&format!("cannot open {src}: {e}")));
    sqlx::query("VACUUM INTO ?")
        .bind(&dest)
        .execute(&db)
        .await
        .unwrap_or_else(|e| fatal_backup(&format!("VACUUM INTO failed: {e}")));
    db.close().await;
    let copy = store::open(&dest)
        .await
        .unwrap_or_else(|e| fatal_backup(&format!("cannot open the copy: {e}")));
    let check: (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(&copy)
        .await
        .unwrap_or_else(|e| fatal_backup(&format!("integrity check failed: {e}")));
    copy.close().await;
    println!("backup {dest} integrity={}", check.0);
    if check.0 != "ok" {
        std::process::exit(1);
    }
}

fn fatal_backup(msg: &str) -> ! {
    eprintln!("identity-service: backup failed: {msg}");
    std::process::exit(1);
}

/// glibc keeps freed large buffers (Argon2 uses 8 MiB per hash) in per-thread arenas, which can
/// add up past the container's memory limit. A fixed mmap threshold returns them to the OS on
/// free, and two arenas cap the per-thread growth. No-op outside glibc (e.g. macOS dev builds).
fn tune_malloc() {
    #[cfg(target_env = "gnu")]
    // SAFETY: mallopt only changes allocator tunables; called once before other threads start.
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, 1 << 20);
        libc::mallopt(libc::M_ARENA_MAX, 2);
    }
}
