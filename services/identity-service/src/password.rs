//! Password hashing with Argon2id. Parameters are sized for a 64 MB container: 8 MiB per hash and
//! at most two hashes at a time (see `http::AppState::hash_slots`).

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

fn argon() -> Argon2<'static> {
    let params = Params::new(8 * 1024, 3, 1, None).expect("argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub fn hash(password: &str) -> String {
    let salt = SaltString::encode_b64(uuid::Uuid::new_v4().as_bytes()).expect("salt");
    argon()
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2 hash")
        .to_string()
}

pub fn verify(password: &str, phc: &str) -> bool {
    PasswordHash::new(phc).is_ok_and(|h| argon().verify_password(password.as_bytes(), &h).is_ok())
}

/// A valid hash of a random password, used to spend the same time on unknown emails.
pub fn dummy_hash() -> String {
    hash(&uuid::Uuid::new_v4().to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn hash_and_verify() {
        let h = super::hash("benar-sekali");
        assert!(h.starts_with("$argon2id$"));
        assert!(super::verify("benar-sekali", &h));
        assert!(!super::verify("salah", &h));
        assert!(!super::verify("benar-sekali", "not-a-hash"));
    }
}
