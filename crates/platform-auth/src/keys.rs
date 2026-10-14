use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{
    SigningKey,
    pkcs8::{DecodePrivateKey, EncodePrivateKey},
};
use rand::rngs::OsRng;

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("invalid key encoding: {0}")]
    Invalid(String),
}

/// Публичный ключ Ed25519 (32 байта).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey {
    pub kid: String,
    pub bytes: [u8; 32],
}

/// Пара ключей подписи токенов (EdDSA).
pub struct KeyPair {
    kid: String,
    signing: SigningKey,
}

impl KeyPair {
    pub fn generate(kid: &str) -> Self {
        Self {
            kid: kid.into(),
            signing: SigningKey::generate(&mut OsRng),
        }
    }

    /// Загрузка приватного ключа из PKCS#8 DER в base64 (формат переменной окружения).
    pub fn from_pkcs8_base64(kid: &str, b64: &str) -> Result<Self, KeyError> {
        let der = STANDARD
            .decode(b64.trim())
            .map_err(|e| KeyError::Invalid(e.to_string()))?;
        let signing =
            SigningKey::from_pkcs8_der(&der).map_err(|e| KeyError::Invalid(e.to_string()))?;
        Ok(Self {
            kid: kid.into(),
            signing,
        })
    }

    pub fn to_pkcs8_base64(&self) -> String {
        STANDARD.encode(self.pkcs8_der())
    }

    pub fn kid(&self) -> &str {
        &self.kid
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey {
            kid: self.kid.clone(),
            bytes: self.signing.verifying_key().to_bytes(),
        }
    }

    pub(crate) fn pkcs8_der(&self) -> Vec<u8> {
        self.signing
            .to_pkcs8_der()
            .expect("encode pkcs8")
            .as_bytes()
            .to_vec()
    }
}
