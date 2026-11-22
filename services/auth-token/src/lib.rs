//! User access tokens (JWT, HS256) for Identitas & Akses.
//!
//! identity-service signs them; core-service only verifies. Both read the same `JWT_SECRET`.
//! Kept tiny on purpose: one algorithm, fixed header, constant-time signature check, and the
//! `alg` from the token is never trusted (prevents `alg: none` and algorithm confusion).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

pub const ISSUER: &str = "kotasiaga-identity";
pub const AUDIENCE: &str = "kotasiaga";
/// Minimum `JWT_SECRET` length in bytes.
pub const MIN_SECRET_LEN: usize = 32;

const HEADER: &str = r#"{"alg":"HS256","typ":"JWT"}"#;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claims {
    /// User id, e.g. `usr_3f2a...`.
    pub sub: String,
    /// Display name, for logs and UI only. Never used for authorization.
    pub name: String,
    pub iss: String,
    pub aud: String,
    /// Issued at, Unix seconds.
    pub iat: i64,
    /// Expiry, Unix seconds.
    pub exp: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TokenError {
    Malformed,
    BadSignature,
    Expired,
    WrongIssuerOrAudience,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Malformed => "token rusak",
            Self::BadSignature => "tanda tangan token tidak valid",
            Self::Expired => "token kedaluwarsa",
            Self::WrongIssuerOrAudience => "token bukan untuk layanan ini",
        })
    }
}

fn mac(secret: &[u8]) -> Hmac<Sha256> {
    // HMAC accepts keys of any length; this cannot fail.
    Hmac::<Sha256>::new_from_slice(secret).expect("hmac key")
}

pub fn sign(claims: &Claims, secret: &[u8]) -> String {
    let payload = serde_json::to_vec(claims).expect("claims serialize");
    let signing_input = format!("{}.{}", B64.encode(HEADER), B64.encode(payload));
    let mut m = mac(secret);
    m.update(signing_input.as_bytes());
    let sig = B64.encode(m.finalize().into_bytes());
    format!("{signing_input}.{sig}")
}

/// Verifies signature, issuer, audience and expiry (`now` in Unix seconds, 30 s leeway).
pub fn verify(token: &str, secret: &[u8], now: i64) -> Result<Claims, TokenError> {
    let mut parts = token.split('.');
    let (Some(h), Some(p), Some(s), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(TokenError::Malformed);
    };
    let header: serde_json::Value = B64
        .decode(h)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or(TokenError::Malformed)?;
    if header.get("alg").and_then(|a| a.as_str()) != Some("HS256") {
        return Err(TokenError::Malformed);
    }
    let sig = B64.decode(s).map_err(|_| TokenError::Malformed)?;
    let mut m = mac(secret);
    m.update(h.as_bytes());
    m.update(b".");
    m.update(p.as_bytes());
    m.verify_slice(&sig).map_err(|_| TokenError::BadSignature)?;
    let claims: Claims = B64
        .decode(p)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or(TokenError::Malformed)?;
    if claims.iss != ISSUER || claims.aud != AUDIENCE {
        return Err(TokenError::WrongIssuerOrAudience);
    }
    if claims.exp + 30 < now {
        return Err(TokenError::Expired);
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"test-secret-test-secret-test-secret!";

    fn claims(exp: i64) -> Claims {
        Claims {
            sub: "usr_1".into(),
            name: "Uji".into(),
            iss: ISSUER.into(),
            aud: AUDIENCE.into(),
            iat: 1000,
            exp,
        }
    }

    #[test]
    fn round_trip() {
        let t = sign(&claims(2000), SECRET);
        assert_eq!(verify(&t, SECRET, 1500), Ok(claims(2000)));
    }

    #[test]
    fn rejects_wrong_secret_expired_and_tampered() {
        let t = sign(&claims(2000), SECRET);
        assert_eq!(
            verify(&t, b"another-secret-another-secret-0000", 1500),
            Err(TokenError::BadSignature)
        );
        assert_eq!(verify(&t, SECRET, 2031), Err(TokenError::Expired));
        let mut evil = claims(2000);
        evil.sub = "usr_admin".into();
        let forged_payload = B64.encode(serde_json::to_vec(&evil).unwrap());
        let parts: Vec<&str> = t.split('.').collect();
        let tampered = format!("{}.{}.{}", parts[0], forged_payload, parts[2]);
        assert_eq!(
            verify(&tampered, SECRET, 1500),
            Err(TokenError::BadSignature)
        );
    }

    #[test]
    fn rejects_alg_none_and_garbage() {
        let payload = B64.encode(serde_json::to_vec(&claims(2000)).unwrap());
        let none = format!("{}.{}.", B64.encode(r#"{"alg":"none"}"#), payload);
        assert_eq!(verify(&none, SECRET, 1500), Err(TokenError::Malformed));
        assert_eq!(verify("abc", SECRET, 1500), Err(TokenError::Malformed));
        assert_eq!(verify("a.b.c.d", SECRET, 1500), Err(TokenError::Malformed));
    }

    #[test]
    fn rejects_other_audience() {
        let mut c = claims(2000);
        c.aud = "someone-else".into();
        let t = sign(&c, SECRET);
        assert_eq!(
            verify(&t, SECRET, 1500),
            Err(TokenError::WrongIssuerOrAudience)
        );
    }
}
