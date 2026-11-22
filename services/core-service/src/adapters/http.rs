//! Resilient HTTP GET for every adapter: timeout, one retry with jitter, size cap, call log.

use std::time::{Duration, Instant};

use serde_json::Value;

/// Upstream responses larger than this are refused (protects the 256 MB container).
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum FetchError {
    Timeout,
    Status(u16),
    Network(String),
    /// The upstream answered, but not in the shape its contract promises.
    Invalid(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => write!(f, "timeout"),
            Self::Status(s) => write!(f, "HTTP {s}"),
            Self::Network(e) => write!(f, "network: {e}"),
            Self::Invalid(e) => write!(f, "invalid response: {e}"),
        }
    }
}

impl FetchError {
    fn retryable(&self) -> bool {
        match self {
            Self::Timeout | Self::Network(_) => true,
            Self::Status(s) => *s >= 500,
            Self::Invalid(_) => false,
        }
    }

    fn log_status(&self) -> String {
        match self {
            Self::Status(s) => s.to_string(),
            Self::Timeout => "timeout".into(),
            Self::Network(_) => "network_error".into(),
            Self::Invalid(_) => "invalid".into(),
        }
    }
}

#[derive(Clone)]
pub struct Upstream {
    client: reqwest::Client,
    timeout: Duration,
}

/// Path part of a URL for the call log (never the query string: it may carry keys).
pub fn log_path(url: &str) -> String {
    reqwest::Url::parse(url)
        .map(|u| u.path().to_string())
        .unwrap_or_else(|_| "?".into())
}

/// One line per outbound call: service, method, path, status, ms, attempt.
pub fn log_call(service: &str, path: &str, status: &str, ms: u64, attempt: u8, mode: &str) {
    tracing::info!(target: "outbound", service, method = "GET", path, status, ms, attempt, mode);
}

fn jitter() -> Duration {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    Duration::from_millis(200 + (nanos % 400) as u64)
}

impl Upstream {
    pub fn new(timeout_ms: u64) -> Self {
        let timeout = Duration::from_millis(timeout_ms);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(timeout)
            .user_agent("kotasiaga-core/0.2 (TST smart city student project)")
            .build()
            .expect("reqwest client");
        Self { client, timeout }
    }

    async fn once(&self, url: &str, headers: &[(&str, &str)]) -> Result<Value, FetchError> {
        let mut req = self.client.get(url).timeout(self.timeout);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let res = req.send().await.map_err(|e| {
            if e.is_timeout() {
                FetchError::Timeout
            } else {
                FetchError::Network(e.to_string())
            }
        })?;
        let status = res.status();
        if !status.is_success() {
            return Err(FetchError::Status(status.as_u16()));
        }
        if res
            .content_length()
            .is_some_and(|n| n as usize > MAX_BODY_BYTES)
        {
            return Err(FetchError::Invalid("body too large".into()));
        }
        let bytes = res.bytes().await.map_err(|e| {
            if e.is_timeout() {
                FetchError::Timeout
            } else {
                FetchError::Network(e.to_string())
            }
        })?;
        if bytes.len() > MAX_BODY_BYTES {
            return Err(FetchError::Invalid("body too large".into()));
        }
        serde_json::from_slice(&bytes).map_err(|e| FetchError::Invalid(format!("not JSON: {e}")))
    }

    /// GET JSON with one retry (after 200-600 ms) on timeout, network error or 5xx.
    pub async fn get_json(
        &self,
        service: &str,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<Value, FetchError> {
        let path = log_path(url);
        let mut attempt = 1u8;
        loop {
            let started = Instant::now();
            let res = self.once(url, headers).await;
            let ms = started.elapsed().as_millis() as u64;
            match &res {
                Ok(_) => log_call(service, &path, "200", ms, attempt, "live"),
                Err(e) => log_call(service, &path, &e.log_status(), ms, attempt, "live"),
            }
            match res {
                Err(e) if attempt == 1 && e.retryable() => {
                    attempt += 1;
                    tokio::time::sleep(jitter()).await;
                }
                other => return other,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_policy() {
        assert!(FetchError::Timeout.retryable());
        assert!(FetchError::Status(503).retryable());
        assert!(!FetchError::Status(404).retryable());
        assert!(!FetchError::Status(401).retryable());
        assert!(!FetchError::Invalid("x".into()).retryable());
    }

    #[test]
    fn log_path_drops_query() {
        assert_eq!(log_path("https://x.test/a/b?api_key=secret"), "/a/b");
    }

    /// A local server that always answers 503: the client tries exactly twice, then gives up.
    #[tokio::test]
    async fn retries_once_on_5xx_then_fails() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering};
        let hits = Arc::new(AtomicU32::new(0));
        let h = hits.clone();
        let app = axum::Router::new().route(
            "/x",
            axum::routing::get(move || {
                let h = h.clone();
                async move {
                    h.fetch_add(1, Ordering::SeqCst);
                    axum::http::StatusCode::SERVICE_UNAVAILABLE
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let up = Upstream::new(2000);
        let r = up.get_json("test", &format!("http://{addr}/x"), &[]).await;
        assert_eq!(r, Err(FetchError::Status(503)));
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn times_out() {
        let app = axum::Router::new().route(
            "/slow",
            axum::routing::get(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                "late"
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let up = Upstream::new(500);
        let started = Instant::now();
        let r = up
            .get_json("test", &format!("http://{addr}/slow"), &[])
            .await;
        assert_eq!(r, Err(FetchError::Timeout));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "two 0.5 s attempts plus jitter"
        );
    }
}
