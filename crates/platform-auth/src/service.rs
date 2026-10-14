//! Межсервисная аутентификация общим секретом (internal-эндпоинты).
use std::sync::Arc;

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header::AUTHORIZATION, request::Parts},
};
use platform_core::error::ApiError;

#[derive(Clone)]
pub struct ServiceToken(String);

impl ServiceToken {
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// Сравнение за постоянное время; пустой секрет не совпадает ни с чем.
    pub fn matches(&self, candidate: &str) -> bool {
        if self.0.is_empty() || self.0.len() != candidate.len() {
            return false;
        }
        self.0
            .bytes()
            .zip(candidate.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }
}

/// Маркер вызывающего сервиса: наличие в аргументах хендлера = проверен сервисный токен.
pub struct ServiceCaller;

impl<S> FromRequestParts<S> for ServiceCaller
where
    S: Send + Sync,
    Arc<ServiceToken>: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let expected = Arc::<ServiceToken>::from_ref(state);
        let given = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(ApiError::unauthorized)?;
        if expected.matches(given) {
            Ok(ServiceCaller)
        } else {
            Err(ApiError::unauthorized())
        }
    }
}
