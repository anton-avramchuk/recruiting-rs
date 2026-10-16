use std::sync::Arc;

use platform_auth::{Audience, Claims, TokenSigner, password};
use platform_db::TenantTx;
use serde::Serialize;
use sqlx::PgPool;

use super::tenants::{DirectoryError, TenantDirectory, TenantStatus};
use crate::{
    domain::{DomainError, Email, User},
    infrastructure::{
        refresh_tokens::{self, Rotation},
        users,
    },
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

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
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

    /// Обмен refresh-токена на новую пару (с ротацией).
    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenPair, AuthFailure> {
        let tenant =
            refresh_tokens::tenant_of(refresh_token).ok_or(AuthFailure::InvalidCredentials)?;
        match self.directory.status(tenant).await? {
            None => return Err(AuthFailure::InvalidCredentials),
            Some(TenantStatus::Suspended) => return Err(AuthFailure::TenantSuspended),
            Some(TenantStatus::Active) => {}
        }

        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        match refresh_tokens::rotate(&mut tx, refresh_token).await? {
            Rotation::Rotated(user_id) => {
                let user = users::find_by_id(&mut tx, user_id)
                    .await?
                    .filter(User::can_log_in)
                    .ok_or(AuthFailure::InvalidCredentials)?;
                let pair = self.issue(&mut tx, &user).await?;
                tx.commit().await?;
                Ok(pair)
            }
            Rotation::Reused(user_id) => {
                // повторное предъявление погашенного токена: отзываем всю цепочку
                refresh_tokens::revoke_all_for_user(&mut tx, user_id).await?;
                tx.commit().await?;
                Err(AuthFailure::InvalidCredentials)
            }
            Rotation::Unknown => Err(AuthFailure::InvalidCredentials),
        }
    }

    /// Выход: отзыв refresh-токена. Идемпотентен.
    pub async fn logout(&self, refresh_token: &str) -> Result<(), AuthFailure> {
        let Some(tenant) = refresh_tokens::tenant_of(refresh_token) else {
            return Ok(());
        };
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        refresh_tokens::revoke(&mut tx, refresh_token).await?;
        tx.commit().await?;
        Ok(())
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
        )
        // имя для подписей действий в других сервисах: отображаемое, а без него - email
        .with_name(if user.display_name.trim().is_empty() {
            user.email.as_str()
        } else {
            user.display_name.as_str()
        });
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
