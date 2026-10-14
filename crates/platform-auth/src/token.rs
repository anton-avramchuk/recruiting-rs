use std::{
    sync::{Mutex, RwLock},
    time::{Duration, Instant},
};

use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode, errors::ErrorKind,
};

use crate::{
    claims::{Audience, Claims},
    jwks,
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

/// Источник ключа проверки: заданный заранее или публикуемый другим сервисом (JWKS).
enum KeySource {
    Fixed(DecodingKey),
    Remote(RemoteKey),
}

struct RemoteKey {
    url: String,
    client: reqwest::Client,
    /// Минимальный интервал между обращениями за ключом: защита от лавины при поддельных токенах.
    min_interval: Duration,
    current: RwLock<Option<DecodingKey>>,
    last_attempt: Mutex<Option<Instant>>,
}

impl RemoteKey {
    /// Разрешает обращение за ключом, если с прошлой попытки прошёл интервал.
    fn may_fetch(&self) -> bool {
        let mut last = self.last_attempt.lock().expect("fetch lock");
        if last.is_some_and(|at| at.elapsed() < self.min_interval) {
            return false;
        }
        *last = Some(Instant::now());
        true
    }

    async fn fetch(&self) -> Result<DecodingKey, String> {
        let doc: serde_json::Value = self
            .client
            .get(&self.url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        let public = jwks::from_jwks(&doc).map_err(|e| e.to_string())?;
        Ok(DecodingKey::from_ed_der(&public.bytes))
    }

    /// Обновляет ключ, если разрешено интервалом. `true` - ключ заменён.
    async fn refresh(&self) -> bool {
        if !self.may_fetch() {
            return false;
        }
        match self.fetch().await {
            Ok(key) => {
                *self.current.write().expect("key lock") = Some(key);
                true
            }
            Err(e) => {
                let _ = e; // недоступность проверяется следующей попыткой после интервала
                false
            }
        }
    }
}

/// Проверяет подпись, срок действия и audience.
pub struct TokenVerifier {
    source: KeySource,
    validation: Validation,
}

impl TokenVerifier {
    pub fn new(public: &PublicKey, audience: Audience) -> Self {
        Self {
            source: KeySource::Fixed(DecodingKey::from_ed_der(&public.bytes)),
            validation: validation(audience),
        }
    }

    /// Верификатор по ключу, который публикует другой сервис (JWKS по `jwks_url`).
    /// Ключ загружается лениво; при неверной подписи он перечитывается не чаще `min_interval`
    /// (так переживается смена ключа без перезапуска). Пока ключа нет, все токены отклоняются.
    pub fn remote(jwks_url: &str, audience: Audience, min_interval: Duration) -> Self {
        Self {
            source: KeySource::Remote(RemoteKey {
                url: jwks_url.to_string(),
                client: reqwest::Client::builder()
                    .timeout(Duration::from_secs(5))
                    .build()
                    .expect("build http client"),
                min_interval,
                current: RwLock::new(None),
                last_attempt: Mutex::new(None),
            }),
            validation: validation(audience),
        }
    }

    /// Синхронная проверка по уже известному ключу (для заданного ключа - всегда).
    /// Удалённый ключ без загруженного значения даёт отказ; для него используйте `verify_async`.
    pub fn verify(&self, token: &str) -> Result<Claims, AuthError> {
        match &self.source {
            KeySource::Fixed(key) => self.decode(token, key).map_err(|(e, _)| e),
            KeySource::Remote(remote) => {
                let guard = remote.current.read().expect("key lock");
                match guard.as_ref() {
                    Some(key) => self.decode(token, key).map_err(|(e, _)| e),
                    None => Err(AuthError::Invalid(
                        "verification key is not available".into(),
                    )),
                }
            }
        }
    }

    pub async fn verify_async(&self, token: &str) -> Result<Claims, AuthError> {
        let KeySource::Remote(remote) = &self.source else {
            return self.verify(token);
        };

        let has_key = remote.current.read().expect("key lock").is_some();
        if !has_key && !remote.refresh().await {
            // ключа нет и получить его сейчас нельзя
            let loaded = remote.current.read().expect("key lock").is_some();
            if !loaded {
                return Err(AuthError::Invalid(
                    "verification key is not available".into(),
                ));
            }
        }

        let first = {
            let guard = remote.current.read().expect("key lock");
            let key = guard
                .as_ref()
                .ok_or_else(|| AuthError::Invalid("verification key is not available".into()))?;
            self.decode(token, key)
        };
        match first {
            Ok(claims) => Ok(claims),
            Err((error, true)) => {
                // подпись не сошлась: возможно, ключ сменился
                if remote.refresh().await {
                    let guard = remote.current.read().expect("key lock");
                    if let Some(key) = guard.as_ref() {
                        return self.decode(token, key).map_err(|(e, _)| e);
                    }
                }
                Err(error)
            }
            Err((error, false)) => Err(error),
        }
    }

    /// Результат проверки; во втором элементе ошибки - «причина в подписи».
    fn decode(&self, token: &str, key: &DecodingKey) -> Result<Claims, (AuthError, bool)> {
        decode::<Claims>(token, key, &self.validation)
            .map(|d| d.claims)
            .map_err(|e| match e.kind() {
                ErrorKind::InvalidAudience => (AuthError::WrongAudience, false),
                ErrorKind::InvalidSignature => (AuthError::Invalid(e.to_string()), true),
                _ => (AuthError::Invalid(e.to_string()), false),
            })
    }
}

fn validation(audience: Audience) -> Validation {
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.set_audience(&[audience.as_str()]);
    validation.set_required_spec_claims(&["exp", "aud"]);
    validation
}
