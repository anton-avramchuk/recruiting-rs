use std::sync::Arc;

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header::AUTHORIZATION, request::Parts},
};
use platform_core::error::ApiError;

use crate::{claims::Claims, role::Permission, token::TokenVerifier};

/// Аутентифицированный субъект запроса. Состояние приложения должно
/// отдавать `Arc<TokenVerifier>` через `FromRef`.
pub struct AuthUser {
    pub claims: Claims,
}

impl AuthUser {
    pub fn require(&self, permission: Permission) -> Result<(), ApiError> {
        if self.claims.has(permission) {
            Ok(())
        } else {
            Err(ApiError::forbidden())
        }
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    Arc<TokenVerifier>: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let verifier = Arc::<TokenVerifier>::from_ref(state);
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(ApiError::unauthorized)?;
        let claims = verifier
            .verify(token)
            .map_err(|_| ApiError::unauthorized())?;
        Ok(AuthUser { claims })
    }
}
