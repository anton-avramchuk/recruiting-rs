use platform_auth::{Role, password};
use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    domain::{DomainError, Email, User, UserStatus},
    infrastructure::users,
};

#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("{0}")]
    Conflict(String),
    #[error("user not found")]
    NotFound,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error("internal: {0}")]
    Internal(String),
}

pub struct NewUser {
    pub email: String,
    pub password: String,
    pub display_name: String,
    pub roles: Vec<Role>,
}

#[derive(Default)]
pub struct UserChanges {
    pub roles: Option<Vec<Role>>,
    pub status: Option<UserStatus>,
    pub display_name: Option<String>,
}

#[derive(Clone)]
pub struct UserService {
    pool: PgPool,
}

impl UserService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(&self, tenant: TenantId) -> Result<Vec<User>, UserError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let list = users::list(&mut tx).await?;
        tx.rollback().await?;
        Ok(list)
    }

    pub async fn create(&self, tenant: TenantId, new: NewUser) -> Result<User, UserError> {
        let email = Email::parse(&new.email)?;
        User::validate_password(&new.password)?;
        let hash = password::hash(&new.password).map_err(|e| UserError::Internal(e.to_string()))?;
        let user = User::register(tenant, email, hash, new.display_name, new.roles)?;

        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        match users::insert(&mut tx, &user).await {
            Ok(()) => {}
            Err(e) if users::is_unique_violation(&e) => {
                return Err(UserError::Conflict("email already registered".into()));
            }
            Err(e) => return Err(e.into()),
        }
        tx.commit().await?;
        Ok(user)
    }

    /// Создаёт владельца тенанта; повторный вызов с тем же email возвращает существующего
    /// пользователя (идемпотентность для саги провижининга). Результат: (пользователь, создан ли).
    pub async fn ensure_owner(
        &self,
        tenant: TenantId,
        mut new: NewUser,
    ) -> Result<(User, bool), UserError> {
        new.roles = vec![Role::Owner];
        let email = Email::parse(&new.email)?;
        {
            let mut tx = TenantTx::begin(&self.pool, tenant).await?;
            let existing = users::find_by_email(&mut tx, &email).await?;
            tx.rollback().await?;
            if let Some(user) = existing {
                return Ok((user, false));
            }
        }
        match self.create(tenant, new).await {
            Ok(user) => Ok((user, true)),
            // гонка двух параллельных вызовов: второй видит победителя
            Err(UserError::Conflict(_)) => {
                let mut tx = TenantTx::begin(&self.pool, tenant).await?;
                let existing = users::find_by_email(&mut tx, &email).await?;
                tx.rollback().await?;
                existing.map(|u| (u, false)).ok_or(UserError::NotFound)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn update(
        &self,
        tenant: TenantId,
        actor: Uuid,
        id: Uuid,
        changes: UserChanges,
    ) -> Result<User, UserError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let mut user = users::find_by_id(&mut tx, id)
            .await?
            .ok_or(UserError::NotFound)?;
        if let Some(roles) = changes.roles {
            user.change_roles(roles)?;
        }
        if let Some(name) = changes.display_name {
            user.display_name = name;
        }
        match changes.status {
            Some(UserStatus::Blocked) if id == actor => {
                return Err(UserError::Conflict("cannot block yourself".into()));
            }
            Some(UserStatus::Blocked) => user.block(),
            Some(UserStatus::Active) => user.unblock(),
            None => {}
        }
        users::update(&mut tx, &user).await?;
        tx.commit().await?;
        Ok(user)
    }
}
