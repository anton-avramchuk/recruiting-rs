use std::sync::OnceLock;

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};

pub fn hash(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
}

pub fn verify(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

/// Проверка против фиктивного хеша: выравнивает время ответа для несуществующего email.
pub fn verify_dummy(password: &str) {
    static DUMMY: OnceLock<String> = OnceLock::new();
    let hash = DUMMY.get_or_init(|| hash("dummy-password").expect("hash dummy"));
    let _ = verify(password, hash);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_verifies_only_the_right_password() {
        let h = hash("correct horse").unwrap();
        assert!(h.starts_with("$argon2"));
        assert!(verify("correct horse", &h));
        assert!(!verify("wrong", &h));
    }

    #[test]
    fn garbage_hash_never_verifies() {
        assert!(!verify("x", "not-a-hash"));
    }
}
