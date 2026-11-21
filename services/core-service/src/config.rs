//! Environment configuration. Mock switches follow AGENTS.md section 7.

use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub database_path: String,
    pub mock: MockConfig,
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
        Self::from_lookup(|k| env::var(k).ok())
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

        Ok(Self {
            port,
            database_path,
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
    fn rejects_bad_bool() {
        assert!(cfg(&[("MOCK_MODE", "yes")]).is_err());
    }
}
