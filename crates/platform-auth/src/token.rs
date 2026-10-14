use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode, errors::ErrorKind,
};

use crate::{
    claims::{Audience, Claims},
    keys::{KeyPair, PublicKey},
};

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("token signing failed: {0}")]
    Sign(String),
    #[error("invalid token: {0}")]
    Invalid(String),
    /// Подпись и срок действительны, но токен выпущен для другой audience.
    #[error("token issued for a different audience")]
    WrongAudience,
}

pub struct TokenSigner {
    key: EncodingKey,
    kid: String,
}

impl TokenSigner {
    pub fn new(pair: &KeyPair) -> Self {
        Self {
            key: EncodingKey::from_ed_der(&pair.pkcs8_der()),
            kid: pair.kid().into(),
        }
    }

    pub fn sign(&self, claims: &Claims) -> Result<String, AuthError> {
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(self.kid.clone());
        encode(&header, claims, &self.key).map_err(|e| AuthError::Sign(e.to_string()))
    }
}

/// Проверяет подпись, срок действия и audience.
pub struct TokenVerifier {
    key: DecodingKey,
    validation: Validation,
}

impl TokenVerifier {
    pub fn new(public: &PublicKey, audience: Audience) -> Self {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_audience(&[audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "aud"]);
        Self {
            key: DecodingKey::from_ed_der(&public.bytes),
            validation,
        }
    }

    pub fn verify(&self, token: &str) -> Result<Claims, AuthError> {
        decode::<Claims>(token, &self.key, &self.validation)
            .map(|d| d.claims)
            .map_err(|e| match e.kind() {
                ErrorKind::InvalidAudience => AuthError::WrongAudience,
                _ => AuthError::Invalid(e.to_string()),
            })
    }
}
