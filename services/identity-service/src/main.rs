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

/// `identity-service backup [dest] [--overwrite]`: consistent online copy with `VACUUM INTO` (safe in WAL mode while the
/// service runs). Works in the distroless image: `docker compose exec identity-service /app backup`.
/// Default destination `/data/backup.db` (replaced each time); prints the path and the result
/// of `PRAGMA integrity_check` on the copy.
async fn backup() {
    let src = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "/data/identity.db".into());
    let args: Vec<String> = std::env::args().skip(2).collect();
    let overwrite = args.iter().any(|a| a == "--overwrite");
    let dest = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "/data/backup.db".into());
    match run_backup(&src, &dest, overwrite).await {
        Ok(line) => println!("{line}"),
        Err(e) => fatal_backup(&e),
    }
}

/// Absolute, symlink-free form of `p`, also when the file does not exist yet.
fn resolve(p: &std::path::Path) -> std::path::PathBuf {
    if let Ok(c) = p.canonicalize() {
        return c;
    }
    let parent = p
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let parent = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    parent.join(p.file_name().unwrap_or_default())
}

/// Refuses a destination that is the live database (or one of its WAL/SHM/journal files), and an
/// existing destination unless `overwrite` is set. The source is never deleted or written.
pub fn check_backup_target(src: &str, dest: &str, overwrite: bool) -> Result<(), String> {
    let (s, d) = (
        resolve(std::path::Path::new(src)),
        resolve(std::path::Path::new(dest)),
    );
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut live = s.clone().into_os_string();
        live.push(suffix);
        if d.as_os_str() == live {
            return Err(format!(
                "destination {dest} is the live database ({src}{suffix}); choose another path"
            ));
        }
    }
    if d.exists() && !overwrite {
        return Err(format!(
            "destination {dest} already exists; remove it or pass --overwrite"
        ));
    }
    Ok(())
}

/// `VACUUM INTO` copy of `src` at `dest` plus `PRAGMA integrity_check` on the copy.
pub async fn run_backup(src: &str, dest: &str, overwrite: bool) -> Result<String, String> {
    check_backup_target(src, dest, overwrite)?;
    if overwrite {
        // Safe: check_backup_target proved `dest` is not the live database or its side files.
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{dest}{suffix}"));
        }
    }
    let db = store::open(src)
        .await
        .map_err(|e| format!("cannot open {src}: {e}"))?;
    let res = sqlx::query("VACUUM INTO ?").bind(dest).execute(&db).await;
    db.close().await;
    res.map_err(|e| format!("VACUUM INTO failed: {e}"))?;
    let copy = store::open(dest)
        .await
        .map_err(|e| format!("cannot open the copy: {e}"))?;
    let check: (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(&copy)
        .await
        .map_err(|e| format!("integrity check failed: {e}"))?;
    copy.close().await;
    if check.0 != "ok" {
        return Err(format!("integrity check of {dest}: {}", check.0));
    }
    Ok(format!("backup {dest} integrity=ok"))
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

#[cfg(test)]
mod backup_tests {
    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ks-backup-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[tokio::test]
    async fn never_touches_the_source() {
        let dir = tmp();
        let src = dir.join("live.db");
        let src_s = src.to_str().unwrap().to_string();
        let db = crate::store::open(&src_s).await.unwrap();
        db.close().await;
        let before = std::fs::read(&src).unwrap();
        for dest in [
            src_s.clone(),
            format!("{src_s}-wal"),
            dir.join("./live.db").to_str().unwrap().to_string(),
        ] {
            for overwrite in [false, true] {
                let err = super::run_backup(&src_s, &dest, overwrite)
                    .await
                    .unwrap_err();
                assert!(err.contains("live database"), "{dest}: {err}");
            }
        }
        assert!(
            src.exists() && std::fs::read(&src).unwrap() == before,
            "source untouched"
        );

        let dest = dir.join("copy.db");
        let dest_s = dest.to_str().unwrap();
        assert!(
            super::run_backup(&src_s, dest_s, false)
                .await
                .unwrap()
                .ends_with("integrity=ok")
        );
        let err = super::run_backup(&src_s, dest_s, false).await.unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        assert!(
            super::run_backup(&src_s, dest_s, true).await.is_ok(),
            "--overwrite replaces a copy"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
