//! Environment configuration for identity-service.

#[derive(Debug, Clone)]
pub struct TestAccount {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub database_path: String,
    /// `None` when unset or too short: auth endpoints then answer 503 instead of crash-looping.
    pub jwt_secret: Option<Vec<u8>>,
    pub access_ttl_s: i64,
    pub refresh_ttl_s: i64,
    pub test_account: Option<TestAccount>,
}

pub const DEFAULT_TEST_EMAIL: &str = "penguji@kotasiaga.test";
pub const DEFAULT_TEST_NAME: &str = "Akun Uji KotaSiaga";

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
        let num = |k: &str, d: i64| -> Result<i64, String> {
            get(k).map_or(Ok(d), |v| {
                v.parse()
                    .map_err(|_| format!("{k} has an invalid value: {v}"))
            })
        };
        let port = num("PORT", 8081)?;
        let port = u16::try_from(port).map_err(|_| format!("PORT out of range: {port}"))?;
        let access_ttl_s = num("ACCESS_TOKEN_TTL_SECONDS", 3600)?;
        if !(60..=86_400).contains(&access_ttl_s) {
            return Err("ACCESS_TOKEN_TTL_SECONDS must be 60..=86400".into());
        }
        let refresh_days = num("REFRESH_TOKEN_TTL_DAYS", 30)?;
        if !(1..=365).contains(&refresh_days) {
            return Err("REFRESH_TOKEN_TTL_DAYS must be 1..=365".into());
        }
        let test_account = match get("TEST_ACCOUNT_PASSWORD") {
            None => None,
            Some(password) => {
                if password.len() < 8 {
                    return Err("TEST_ACCOUNT_PASSWORD must be at least 8 characters".into());
                }
                auth_token::reject_placeholder("TEST_ACCOUNT_PASSWORD", &password)?;
                Some(TestAccount {
                    email: get("TEST_ACCOUNT_EMAIL")
                        .unwrap_or_else(|| DEFAULT_TEST_EMAIL.into())
                        .to_lowercase(),
                    password,
                    display_name: get("TEST_ACCOUNT_NAME")
                        .unwrap_or_else(|| DEFAULT_TEST_NAME.into()),
                })
            }
        };
        if let Some(s) = get("JWT_SECRET") {
            auth_token::reject_placeholder("JWT_SECRET", &s)?;
        }
        Ok(Self {
            port,
            database_path: get("DATABASE_PATH").unwrap_or_else(|| "/data/identity.db".into()),
            jwt_secret: get("JWT_SECRET")
                .filter(|s| s.len() >= auth_token::MIN_SECRET_LEN)
                .map(String::into_bytes),
            access_ttl_s,
            refresh_ttl_s: refresh_days * 86_400,
            test_account,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(pairs: &[(&str, &str)]) -> Result<Config, String> {
        let m: std::collections::HashMap<_, _> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(|k| m.get(k).cloned())
    }

    #[test]
    fn short_secret_counts_as_unset() {
        assert!(
            cfg(&[("JWT_SECRET", "short")])
                .unwrap()
                .jwt_secret
                .is_none()
        );
        let long = "x".repeat(32);
        assert!(cfg(&[("JWT_SECRET", &long)]).unwrap().jwt_secret.is_some());
    }

    #[test]
    fn placeholder_secrets_refuse_to_start() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env.example");
        let text = std::fs::read_to_string(path).unwrap();
        let mut checked = 0;
        for line in text.lines().filter(|l| !l.trim_start().starts_with('#')) {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            if ["JWT_SECRET", "TEST_ACCOUNT_PASSWORD"].contains(&k) {
                let err = cfg(&[(k, v)]).unwrap_err();
                assert!(err.contains("placeholder"), "{k}: {err}");
                checked += 1;
            }
        }
        assert_eq!(checked, 2);
    }

    #[test]
    fn test_account_only_with_password() {
        assert!(cfg(&[]).unwrap().test_account.is_none());
        let pw = format!("pw-{}", uuid::Uuid::new_v4().simple());
        let c = cfg(&[("TEST_ACCOUNT_PASSWORD", &pw)]).unwrap();
        assert_eq!(c.test_account.unwrap().email, DEFAULT_TEST_EMAIL);
        assert!(cfg(&[("TEST_ACCOUNT_PASSWORD", "short")]).is_err());
    }
}
