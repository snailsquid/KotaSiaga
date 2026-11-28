//! The only HTTP client to other services (AGENTS.md section 6): 5 s timeout, one retry with
//! 200-600 ms jitter on timeout, network error or 5xx (never on 4xx, never for calls that are
//! not safe to repeat), 4 MB body cap, and one `outbound` log line per attempt with timestamp,
//! service, method, path (no query string), status and ms. Keys never reach the log.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum FetchError {
    Timeout,
    Network(String),
    /// Answered, but not with the JSON its contract promises.
    Invalid(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => write!(f, "timeout"),
            Self::Network(e) => write!(f, "network: {e}"),
            Self::Invalid(e) => write!(f, "invalid response: {e}"),
        }
    }
}

/// An HTTP answer (any status). `body` is `Null` when a non-2xx answer was not JSON.
#[derive(Debug, Clone)]
pub struct Resp {
    pub status: u16,
    pub body: Value,
    pub retry_after: Option<String>,
}

impl Resp {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Demo faults (MOCK_MODE): upstream name -> down until.
#[derive(Default)]
pub struct Faults(Mutex<HashMap<String, Instant>>);

impl Faults {
    pub fn set(&self, upstream: &str, for_: Duration) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(upstream.to_string(), Instant::now() + for_);
    }

    pub fn clear(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    pub fn is_down(&self, upstream: &str) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(upstream)
            .is_some_and(|t| *t > Instant::now())
    }

    pub fn down(&self) -> Vec<String> {
        let now = Instant::now();
        let mut v: Vec<String> = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|(_, t)| **t > now)
            .map(|(k, _)| k.clone())
            .collect();
        v.sort();
        v
    }
}

/// Path part of a URL (never the query string).
pub fn log_path(url: &str) -> String {
    reqwest::Url::parse(url)
        .map(|u| u.path().to_string())
        .unwrap_or_else(|_| "?".into())
}

/// One line per outbound call. This log is the cross-group call evidence ("bukti").
pub fn log_call(
    service: &str,
    method: &str,
    path: &str,
    status: &str,
    ms: u64,
    attempt: u8,
    mode: &str,
) {
    tracing::info!(target: "outbound", service, method, path, status, ms, attempt, mode);
}

fn jitter() -> Duration {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    Duration::from_millis(200 + (nanos % 400) as u64)
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    timeout: Duration,
}

impl Client {
    pub fn new(timeout_ms: u64) -> Self {
        let timeout = Duration::from_millis(timeout_ms);
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(timeout)
            .user_agent("kotasiaga-integration/0.1 (TST x PWAM student project)")
            .build()
            .expect("reqwest client");
        Self { http, timeout }
    }

    async fn once(
        &self,
        method: &reqwest::Method,
        url: &str,
        headers: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<Resp, FetchError> {
        let mut req = self
            .http
            .request(method.clone(), url)
            .timeout(self.timeout)
            .header("Accept", "application/json");
        for (k, v) in headers {
            req = req.header(*k, v.as_str());
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let net = |e: reqwest::Error| {
            if e.is_timeout() {
                FetchError::Timeout
            } else {
                FetchError::Network(e.to_string())
            }
        };
        let res = req.send().await.map_err(net)?;
        let status = res.status().as_u16();
        let retry_after = res
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        if res
            .content_length()
            .is_some_and(|n| n as usize > MAX_BODY_BYTES)
        {
            return Err(FetchError::Invalid("body too large".into()));
        }
        let bytes = res.bytes().await.map_err(net)?;
        if bytes.len() > MAX_BODY_BYTES {
            return Err(FetchError::Invalid("body too large".into()));
        }
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(e) if (200..300).contains(&status) => {
                    return Err(FetchError::Invalid(format!("not JSON: {e}")));
                }
                Err(_) => Value::Null,
            }
        };
        Ok(Resp {
            status,
            body,
            retry_after,
        })
    }

    /// One call with the retry policy. `retry`: false for calls that must not run twice
    /// (creating a report, a vote, an account). `faults`: a demo fault turns the call into
    /// a simulated outage without touching the network.
    #[allow(clippy::too_many_arguments)]
    pub async fn send(
        &self,
        service: &str,
        method: reqwest::Method,
        url: &str,
        headers: &[(&str, String)],
        body: Option<&Value>,
        retry: bool,
        faults: &Faults,
    ) -> Result<Resp, FetchError> {
        let path = log_path(url);
        if faults.is_down(service) {
            log_call(
                service,
                method.as_str(),
                &path,
                "simulated_down",
                0,
                1,
                "mock",
            );
            return Err(FetchError::Network("simulated outage (mock fault)".into()));
        }
        let mut attempt = 1u8;
        loop {
            let started = Instant::now();
            let res = self.once(&method, url, headers, body).await;
            let ms = started.elapsed().as_millis() as u64;
            let (status, again) = match &res {
                Ok(r) => (r.status.to_string(), r.status >= 500),
                Err(FetchError::Timeout) => ("timeout".into(), true),
                Err(FetchError::Network(_)) => ("network_error".into(), true),
                Err(FetchError::Invalid(_)) => ("invalid".into(), false),
            };
            log_call(
                service,
                method.as_str(),
                &path,
                &status,
                ms,
                attempt,
                "live",
            );
            if again && retry && attempt == 1 {
                attempt += 1;
                tokio::time::sleep(jitter()).await;
                continue;
            }
            return res;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    async fn serve(app: axum::Router) -> String {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let a = l.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        format!("http://{a}")
    }

    fn counting(status: axum::http::StatusCode) -> (axum::Router, Arc<AtomicU32>) {
        let hits = Arc::new(AtomicU32::new(0));
        let h = hits.clone();
        let app = axum::Router::new().route(
            "/x",
            axum::routing::any(move || {
                let h = h.clone();
                async move {
                    h.fetch_add(1, Ordering::SeqCst);
                    (
                        status,
                        axum::Json(serde_json::json!({"error": {"code": "x"}})),
                    )
                }
            }),
        );
        (app, hits)
    }

    #[tokio::test]
    async fn retries_once_on_5xx() {
        let (app, hits) = counting(axum::http::StatusCode::BAD_GATEWAY);
        let base = serve(app).await;
        let r = Client::new(2000)
            .send(
                "t",
                reqwest::Method::GET,
                &format!("{base}/x"),
                &[],
                None,
                true,
                &Faults::default(),
            )
            .await
            .unwrap();
        assert_eq!(r.status, 502);
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn never_retries_4xx_or_unsafe_calls() {
        let (app, hits) = counting(axum::http::StatusCode::UNAUTHORIZED);
        let base = serve(app).await;
        let c = Client::new(2000);
        let r = c
            .send(
                "t",
                reqwest::Method::GET,
                &format!("{base}/x"),
                &[],
                None,
                true,
                &Faults::default(),
            )
            .await
            .unwrap();
        assert_eq!(r.status, 401);
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        let (app, hits) = counting(axum::http::StatusCode::SERVICE_UNAVAILABLE);
        let base = serve(app).await;
        c.send(
            "t",
            reqwest::Method::POST,
            &format!("{base}/x"),
            &[],
            None,
            false,
            &Faults::default(),
        )
        .await
        .unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn times_out_after_two_attempts() {
        let app = axum::Router::new().route(
            "/slow",
            axum::routing::get(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                "late"
            }),
        );
        let base = serve(app).await;
        let started = Instant::now();
        let r = Client::new(500)
            .send(
                "t",
                reqwest::Method::GET,
                &format!("{base}/slow"),
                &[],
                None,
                true,
                &Faults::default(),
            )
            .await;
        assert_eq!(r.unwrap_err(), FetchError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[tokio::test]
    async fn faults_short_circuit() {
        let f = Faults::default();
        f.set("tourism", Duration::from_secs(30));
        assert!(f.is_down("tourism"));
        assert_eq!(f.down(), vec!["tourism".to_string()]);
        let r = Client::new(500)
            .send(
                "tourism",
                reqwest::Method::GET,
                "http://127.0.0.1:9/x",
                &[],
                None,
                true,
                &f,
            )
            .await;
        assert!(matches!(r, Err(FetchError::Network(_))));
        f.clear();
        assert!(!f.is_down("tourism"));
    }

    #[test]
    fn log_path_drops_query() {
        assert_eq!(
            log_path("https://x.test/api/feed?api_key=secret"),
            "/api/feed"
        );
    }
}
