//! Configuration from the environment (`.env`, git-ignored). Every value has a safe default
//! except the keys, which are only ever read from the environment and never logged.

use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Upstream {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    /// Serve the generated mock (in the partner's own format) instead of calling it.
    pub mock: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    /// KotaSiaga microservice on the STB, e.g. `https://2bintang1hati.stei.site/api`.
    pub kotasiaga_base_url: String,
    /// Our group key for KotaSiaga's `X-API-Key` (risk lookups, routes, keyed reads).
    pub kotasiaga_api_key: Option<String>,
    pub tourism: Upstream,
    pub health: Upstream,
    /// `X-API-Key` value -> caller name, for server-to-server callers of this service.
    pub api_keys: HashMap<String, String>,
    /// Admin key for `/_mock/*`.
    pub admin_api_key: Option<String>,
    pub mock_mode: bool,
    pub mock_seed: u64,
    pub timeout_ms: u64,
    pub partner_refresh_s: u64,
    pub risk_refresh_s: u64,
    pub rate_limit_per_minute: u32,
}

const MIN_KEY_LEN: usize = 16;

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|k| {
            std::env::var(k)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        })
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let url = |k: &str| get(k).map(|u| u.trim_end_matches('/').to_string());
        let mock_mode = opt_bool(&get, "MOCK_MODE")?.unwrap_or(false);
        let partner = |base: &str, key: &str, mock: &str| -> Result<Upstream, String> {
            let base_url = url(base);
            let forced = opt_bool(&get, mock)?.unwrap_or(mock_mode);
            Ok(Upstream {
                mock: forced || base_url.is_none(),
                base_url,
                api_key: get(key),
            })
        };
        let mut api_keys = HashMap::new();
        for pair in get("API_KEYS").unwrap_or_default().split(',') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            let (name, key) = pair
                .split_once(':')
                .ok_or_else(|| format!("API_KEYS entry must be name:key, got {pair:?}"))?;
            let (name, key) = (name.trim(), key.trim());
            if key.len() < MIN_KEY_LEN || key.starts_with("CHANGE_ME") {
                return Err(format!(
                    "API_KEYS key for {name} must be at least {MIN_KEY_LEN} random characters"
                ));
            }
            if name == "admin" {
                return Err("API_KEYS cannot use the name admin".into());
            }
            if api_keys.insert(key.to_string(), name.to_string()).is_some() {
                return Err("API_KEYS has the same key twice".into());
            }
        }
        let admin_api_key = get("ADMIN_API_KEY");
        if let Some(k) = &admin_api_key
            && (k.len() < MIN_KEY_LEN || k.starts_with("CHANGE_ME"))
        {
            return Err(format!(
                "ADMIN_API_KEY must be at least {MIN_KEY_LEN} random characters"
            ));
        }
        Ok(Self {
            port: num(&get, "PORT", 8090)?,
            kotasiaga_base_url: url("KOTASIAGA_BASE_URL")
                .unwrap_or_else(|| "https://2bintang1hati.stei.site/api".into()),
            kotasiaga_api_key: get("KOTASIAGA_API_KEY"),
            tourism: partner(
                "TOURISM_BASE_URL",
                "TOURISM_API_KEY",
                "MOCK_UPSTREAM_TOURISM",
            )?,
            health: partner("HEALTH_BASE_URL", "HEALTH_API_KEY", "MOCK_UPSTREAM_HEALTH")?,
            api_keys,
            admin_api_key,
            mock_mode,
            mock_seed: match get("MOCK_SEED") {
                Some(s) => s.parse().map_err(|_| "MOCK_SEED must be an integer")?,
                None => random_seed(),
            },
            timeout_ms: num(&get, "UPSTREAM_TIMEOUT_MS", 5000)?.clamp(500, 30_000),
            partner_refresh_s: num(&get, "PARTNER_REFRESH_SECONDS", 300)?.max(10),
            risk_refresh_s: num(&get, "RISK_REFRESH_SECONDS", 30)?.max(5),
            rate_limit_per_minute: num(&get, "RATE_LIMIT_PER_MINUTE", 600)?.max(10),
        })
    }

    /// `admin`, a caller name from `API_KEYS`, or `None`.
    pub fn caller(&self, key: &str) -> Option<&str> {
        if self.admin_api_key.as_deref() == Some(key) {
            return Some("admin");
        }
        self.api_keys.get(key).map(String::as_str)
    }
}

fn num<T: std::str::FromStr>(
    get: &impl Fn(&str) -> Option<String>,
    key: &str,
    default: T,
) -> Result<T, String> {
    match get(key) {
        Some(v) => v
            .parse()
            .map_err(|_| format!("{key} has an invalid value: {v}")),
        None => Ok(default),
    }
}

fn opt_bool(get: &impl Fn(&str) -> Option<String>, key: &str) -> Result<Option<bool>, String> {
    match get(key).as_deref() {
        None | Some("") => Ok(None),
        Some("true") | Some("1") => Ok(Some(true)),
        Some("false") | Some("0") => Ok(Some(false)),
        Some(v) => Err(format!("{key} must be true or false, got {v}")),
    }
}

fn random_seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    (nanos as u64) ^ ((std::process::id() as u64) << 32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(pairs: &[(&str, &str)]) -> Result<Config, String> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(|k| m.get(k).cloned())
    }

    #[test]
    fn defaults() {
        let c = cfg(&[]).unwrap();
        assert_eq!(c.port, 8090);
        assert_eq!(c.kotasiaga_base_url, "https://2bintang1hati.stei.site/api");
        assert_eq!(c.timeout_ms, 5000);
        assert!(c.tourism.mock && c.health.mock, "no base URL = mocked");
    }

    #[test]
    fn partners_go_live_with_a_base_url_unless_mocked() {
        let c = cfg(&[
            ("TOURISM_BASE_URL", "https://chitatomigoreng.stei.site/api/"),
            ("HEALTH_BASE_URL", "https://donorkita.stei.site/api"),
        ])
        .unwrap();
        assert!(!c.tourism.mock && !c.health.mock);
        assert_eq!(
            c.tourism.base_url.as_deref(),
            Some("https://chitatomigoreng.stei.site/api")
        );
        let c = cfg(&[
            ("TOURISM_BASE_URL", "https://x.test/api"),
            ("HEALTH_BASE_URL", "https://y.test/api"),
            ("MOCK_MODE", "true"),
            ("MOCK_UPSTREAM_HEALTH", "false"),
        ])
        .unwrap();
        assert!(c.tourism.mock, "MOCK_MODE mocks partners by default");
        assert!(!c.health.mock, "an explicit MOCK_UPSTREAM_* wins");
    }

    #[test]
    fn rejects_weak_or_duplicate_keys() {
        assert!(cfg(&[("API_KEYS", "app:short")]).is_err());
        assert!(cfg(&[("API_KEYS", "app:CHANGE_ME_16_CHARS_MIN")]).is_err());
        assert!(cfg(&[("API_KEYS", "a:aaaaaaaaaaaaaaaa,b:aaaaaaaaaaaaaaaa")]).is_err());
        assert!(cfg(&[("ADMIN_API_KEY", "CHANGE_ME_ADMIN_KEY")]).is_err());
        let c = cfg(&[
            ("API_KEYS", "demo:aaaaaaaaaaaaaaaa"),
            ("ADMIN_API_KEY", "bbbbbbbbbbbbbbbb"),
        ])
        .unwrap();
        assert_eq!(c.caller("aaaaaaaaaaaaaaaa"), Some("demo"));
        assert_eq!(c.caller("bbbbbbbbbbbbbbbb"), Some("admin"));
        assert_eq!(c.caller("nope"), None);
    }

    /// The shipped example must start as-is (blank keys = unset) and point at the real hosts.
    #[test]
    fn env_example_loads() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../.env.integration.example"
        );
        let text = std::fs::read_to_string(path).unwrap();
        let pairs: Vec<(&str, &str)> = text
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .filter_map(|l| l.split_once('='))
            .filter(|(_, v)| !v.trim().is_empty())
            .collect();
        let c = cfg(&pairs).unwrap();
        assert_eq!(c.kotasiaga_base_url, "https://2bintang1hati.stei.site/api");
        assert_eq!(
            c.tourism.base_url.as_deref(),
            Some("https://chitatomigoreng.stei.site/api")
        );
        assert_eq!(
            c.health.base_url.as_deref(),
            Some("https://donorkita.stei.site/api")
        );
        assert!(!c.tourism.mock && !c.health.mock);
    }
}
