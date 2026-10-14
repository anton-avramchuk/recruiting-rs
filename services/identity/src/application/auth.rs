use std::sync::Arc;

use platform_auth::{Audience, Claims, TokenSigner};
use platform_db::TenantTx;
use serde::Serialize;
use sqlx::PgPool;

use super::tenants::{DirectoryError, TenantDirectory, TenantStatus};
use crate::{
    domain::{DomainError, Email, User},
    infrastructure::{password, refresh_tokens, users},
};

#[derive(Debug, thiserror::Error)]
pub enum AuthFailure {
    /// Неверный email/пароль, неизвестный тенант, заблокированный пользователь.
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("tenant suspended")]
    TenantSuspended,
    #[error(transparent)]
    Directory(#[from] DirectoryError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error("internal: {0}")]
    Internal(String),
}

impl From<DomainError> for AuthFailure {
    fn from(_: DomainError) -> Self {
        AuthFailure::InvalidCredentials
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct TokenLifetimes {
    pub access_secs: i64,
    pub refresh_secs: i64,
}

impl Default for TokenLifetimes {
    fn default() -> Self {
        Self {
            access_secs: 600,
            refresh_secs: 30 * 24 * 3600,
        }
    }
}

#[derive(Clone)]
pub struct AuthService {
    pub(crate) pool: PgPool,
    pub(crate) directory: Arc<dyn TenantDirectory>,
    pub(crate) signer: Arc<TokenSigner>,
    pub(crate) lifetimes: TokenLifetimes,
}

impl AuthService {
    pub fn new(
        pool: PgPool,
        directory: Arc<dyn TenantDirectory>,
        signer: Arc<TokenSigner>,
        lifetimes: TokenLifetimes,
    ) -> Self {
        Self {
            pool,
            directory,
            signer,
            lifetimes,
        }
    }

    pub async fn login(
        &self,
        tenant_slug: &str,
        email: &str,
        password_plain: &str,
    ) -> Result<TokenPair, AuthFailure> {
        let Some(tenant) = self.directory.resolve(tenant_slug).await? else {
            password::verify_dummy(password_plain);
            return Err(AuthFailure::InvalidCredentials);
        };
        if tenant.status == TenantStatus::Suspended {
            return Err(AuthFailure::TenantSuspended);
        }
        let Ok(email) = Email::parse(email) else {
            password::verify_dummy(password_plain);
            return Err(AuthFailure::InvalidCredentials);
        };

        let mut tx = TenantTx::begin(&self.pool, tenant.id).await?;
        let user = users::find_by_email(&mut tx, &email).await?;
        let user = match user {
            Some(u) if password::verify(password_plain, &u.password_hash) && u.can_log_in() => u,
            Some(_) => return Err(AuthFailure::InvalidCredentials),
            None => {
                password::verify_dummy(password_plain);
                return Err(AuthFailure::InvalidCredentials);
            }
        };
        let pair = self.issue(&mut tx, &user).await?;
        tx.commit().await?;
        Ok(pair)
    }

    /// Выпускает пару токенов и сохраняет хеш refresh-токена в транзакции вызывающего.
    pub(crate) async fn issue(
        &self,
        tx: &mut TenantTx<'_>,
        user: &User,
    ) -> Result<TokenPair, AuthFailure> {
        let claims = Claims::new(
            user.id,
            Some(user.tenant_id),
            user.roles.clone(),
            Audience::Tenant,
            self.lifetimes.access_secs,
        );
        let access_token = self
            .signer
            .sign(&claims)
            .map_err(|e| AuthFailure::Internal(e.to_string()))?;
        let refresh_token = refresh_tokens::generate(user.tenant_id);
        refresh_tokens::insert(
            tx,
            user.tenant_id,
            user.id,
            &refresh_token,
            self.lifetimes.refresh_secs,
        )
        .await?;
        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in: self.lifetimes.access_secs,
        })
    }
}
