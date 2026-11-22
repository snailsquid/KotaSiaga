//! Environment configuration. Mock switches follow AGENTS.md section 7.

use std::collections::HashMap;
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub database_path: String,
    pub mock: MockConfig,
    /// `X-API-Key` value -> caller name (e.g. `tourism`). From `API_KEYS=name:key,name:key`.
    pub api_keys: HashMap<String, String>,
    /// Key for `/_mock/*` (and later admin actions). From `ADMIN_API_KEY`.
    pub admin_api_key: Option<String>,
    /// Shared with identity-service to verify user JWTs. `None` = every Bearer token is rejected.
    pub jwt_secret: Option<Vec<u8>>,
    pub upstreams: UpstreamConfig,
}

/// Base URLs and refresh intervals for the Adaptor Data Eksternal (anti-corruption layer).
#[derive(Debug, Clone)]
pub struct UpstreamConfig {
    pub timeout_ms: u64,
    pub open_meteo_url: String,
    pub petabencana_url: String,
    pub osrm_url: String,
    /// In MOCK_MODE, still ask the real OSRM for road geometry (risk stays mock-driven).
    pub osrm_in_mock: bool,
    pub tourism_base_url: Option<String>,
    pub tourism_api_key: Option<String>,
    pub health_base_url: Option<String>,
    pub health_events_path: String,
    pub health_api_key: Option<String>,
    pub weather_refresh_s: u64,
    pub disaster_refresh_s: u64,
    pub partner_refresh_s: u64,
}

impl Config {
    pub fn caller(&self, key: &str) -> Option<&str> {
        if self.admin_api_key.as_deref() == Some(key) {
            return Some("admin");
        }
        self.api_keys.get(key).map(String::as_str)
    }
}

#[derive(Debug, Clone)]
pub struct MockConfig {
    pub enabled: bool,
    pub tick_seconds: u64,
    pub time_scale: u32,
    pub seed: u64,
    pub upstream_tourism: bool,
    pub upstream_health: bool,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        // Blank values count as unset: a deploy platform may write `KEY=` lines to `.env`.
        Self::from_lookup(|k| {
            env::var(k)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        })
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let port = parse_or(&get, "PORT", 8080u16)?;
        let database_path = get("DATABASE_PATH").unwrap_or_else(|| "/data/core.db".into());

        let tick_seconds = parse_or(&get, "MOCK_TICK_SECONDS", 5u64)?;
        if !(2..=60).contains(&tick_seconds) {
            return Err(format!(
                "MOCK_TICK_SECONDS must be 2..=60, got {tick_seconds}"
            ));
        }
        let time_scale = parse_or(&get, "MOCK_TIME_SCALE", 60u32)?;
        if time_scale == 0 {
            return Err("MOCK_TIME_SCALE must be >= 1".into());
        }
        let seed = match get("MOCK_SEED") {
            Some(s) => s
                .parse()
                .map_err(|_| format!("MOCK_SEED is not a u64: {s}"))?,
            None => random_seed(),
        };

        let mut api_keys = HashMap::new();
        for pair in get("API_KEYS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            let (name, key) = pair
                .split_once(':')
                .ok_or("API_KEYS entries must be name:key")?;
            if name.trim().eq_ignore_ascii_case("admin") {
                return Err(
                    "API_KEYS must not use the name admin; set ADMIN_API_KEY instead".into(),
                );
            }
            if key.len() < 16 {
                return Err(format!("API key for {name} must be at least 16 characters"));
            }
            api_keys.insert(key.to_string(), name.to_string());
        }
        let admin_api_key = get("ADMIN_API_KEY").filter(|k| !k.is_empty());
        if admin_api_key.as_ref().is_some_and(|k| k.len() < 16) {
            return Err("ADMIN_API_KEY must be at least 16 characters".into());
        }

        let url = |k: &str, d: &str| {
            get(k)
                .unwrap_or_else(|| d.to_string())
                .trim_end_matches('/')
                .to_string()
        };
        let upstreams = UpstreamConfig {
            timeout_ms: parse_or(&get, "UPSTREAM_TIMEOUT_MS", 5000u64)?.clamp(500, 30_000),
            open_meteo_url: url("OPEN_METEO_URL", "https://api.open-meteo.com"),
            petabencana_url: url("PETABENCANA_URL", "https://data.petabencana.id"),
            osrm_url: url("OSRM_URL", "https://router.project-osrm.org"),
            osrm_in_mock: parse_bool(&get, "OSRM_IN_MOCK")?,
            tourism_base_url: get("TOURISM_BASE_URL").map(|u| u.trim_end_matches('/').to_string()),
            tourism_api_key: get("TOURISM_API_KEY"),
            health_base_url: get("HEALTH_BASE_URL").map(|u| u.trim_end_matches('/').to_string()),
            health_events_path: get("HEALTH_EVENTS_PATH")
                .unwrap_or_else(|| "/donation-events".into()),
            health_api_key: get("HEALTH_API_KEY"),
            weather_refresh_s: parse_or(&get, "WEATHER_REFRESH_SECONDS", 600u64)?.max(60),
            disaster_refresh_s: parse_or(&get, "DISASTER_REFRESH_SECONDS", 300u64)?.max(60),
            partner_refresh_s: parse_or(&get, "PARTNER_REFRESH_SECONDS", 300u64)?.max(10),
        };
        Ok(Self {
            port,
            database_path,
            api_keys,
            admin_api_key,
            jwt_secret: get("JWT_SECRET")
                .filter(|s| s.len() >= auth_token::MIN_SECRET_LEN)
                .map(String::into_bytes),
            upstreams,
            mock: MockConfig {
                enabled: parse_bool(&get, "MOCK_MODE")?,
                tick_seconds,
                time_scale,
                seed,
                upstream_tourism: parse_bool(&get, "MOCK_UPSTREAM_TOURISM")?,
                upstream_health: parse_bool(&get, "MOCK_UPSTREAM_HEALTH")?,
            },
        })
    }
}

fn parse_or<T: std::str::FromStr>(
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

fn parse_bool(get: &impl Fn(&str) -> Option<String>, key: &str) -> Result<bool, String> {
    match get(key).as_deref() {
        None | Some("false") | Some("0") | Some("") => Ok(false),
        Some("true") | Some("1") => Ok(true),
        Some(v) => Err(format!("{key} must be true or false, got {v}")),
    }
}

fn random_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    (nanos as u64) ^ ((std::process::id() as u64) << 32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cfg(pairs: &[(&str, &str)]) -> Result<Config, String> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(|k| m.get(k).cloned())
    }

    #[test]
    fn defaults_match_agents_md() {
        let c = cfg(&[("MOCK_SEED", "7")]).unwrap();
        assert_eq!(c.port, 8080);
        assert!(!c.mock.enabled);
        assert_eq!(c.mock.tick_seconds, 5);
        assert_eq!(c.mock.time_scale, 60);
        assert_eq!(c.mock.seed, 7);
    }

    #[test]
    fn rejects_tick_out_of_range() {
        assert!(cfg(&[("MOCK_TICK_SECONDS", "1")]).is_err());
        assert!(cfg(&[("MOCK_TICK_SECONDS", "61")]).is_err());
    }

    #[test]
    fn parses_api_keys() {
        let c = cfg(&[
            (
                "API_KEYS",
                "tourism:aaaaaaaaaaaaaaaa, health:bbbbbbbbbbbbbbbb",
            ),
            ("ADMIN_API_KEY", "cccccccccccccccc"),
        ])
        .unwrap();
        assert_eq!(c.caller("aaaaaaaaaaaaaaaa"), Some("tourism"));
        assert_eq!(c.caller("bbbbbbbbbbbbbbbb"), Some("health"));
        assert_eq!(c.caller("cccccccccccccccc"), Some("admin"));
        assert_eq!(c.caller("nope"), None);
        assert!(cfg(&[("API_KEYS", "short:abc")]).is_err());
        assert!(cfg(&[("API_KEYS", "nokey")]).is_err());
    }

    #[test]
    fn api_keys_cannot_claim_the_admin_name() {
        for name in ["admin", "Admin", " ADMIN"] {
            let err = cfg(&[("API_KEYS", &format!("{name}:dddddddddddddddd"))]).unwrap_err();
            assert!(err.contains("ADMIN_API_KEY"), "{name}: {err}");
        }
    }

    #[test]
    fn rejects_bad_bool() {
        assert!(cfg(&[("MOCK_MODE", "yes")]).is_err());
    }
}
