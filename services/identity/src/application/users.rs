use grid_contracts::{GridPage, GridQuery, SortKey};
use platform_auth::{Role, password};
use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    domain::{DomainError, Email, User, UserStatus},
    infrastructure::{refresh_tokens, roles, users},
};

impl From<platform_values::ValueError> for UserError {
    fn from(e: platform_values::ValueError) -> Self {
        UserError::Domain(e.into())
    }
}

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
    #[error(transparent)]
    Grid(#[from] platform_grid::GridError),
}

impl From<platform_grid::FetchError> for UserError {
    fn from(e: platform_grid::FetchError) -> Self {
        match e {
            platform_grid::FetchError::Grid(e) => UserError::Grid(e),
            platform_grid::FetchError::Db(e) => UserError::Db(e),
        }
    }
}

pub struct NewUser {
    pub email: String,
    pub password: String,
    pub display_name: String,
    pub roles: Vec<Role>,
    /// Собственная роль организации вместо встроенных.
    pub role_id: Option<Uuid>,
}

#[derive(Default)]
pub struct UserChanges {
    pub roles: Option<Vec<Role>>,
    pub role_id: Option<Uuid>,
    pub status: Option<UserStatus>,
    pub display_name: Option<String>,
    /// Новый пароль (открытый текст) - сброс владельцем.
    pub password: Option<String>,
}

#[derive(Clone)]
pub struct UserService {
    pool: PgPool,
}

impl UserService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Страница пользователей организации; сортировка по умолчанию - по email.
    pub async fn list(
        &self,
        tenant: TenantId,
        mut query: GridQuery,
    ) -> Result<GridPage<User>, UserError> {
        if query.sort.is_empty() {
            query.sort.push(SortKey::asc("email"));
        }
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let page = users::page(&mut tx, &query).await;
        tx.rollback().await?;
        Ok(page?)
    }

    pub async fn get(&self, tenant: TenantId, id: Uuid) -> Result<User, UserError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let user = users::find_by_id(&mut tx, id).await?;
        tx.rollback().await?;
        user.ok_or(UserError::NotFound)
    }

    pub async fn create(&self, tenant: TenantId, new: NewUser) -> Result<User, UserError> {
        let email = Email::parse(&new.email)?;
        User::validate_password(&new.password)?;
        let hash = password::hash(&new.password).map_err(|e| UserError::Internal(e.to_string()))?;
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let user = match new.role_id {
            Some(role_id) => {
                if !new.roles.is_empty() {
                    return Err(DomainError::AmbiguousRole.into());
                }
                let role = roles::find(&mut tx, role_id)
                    .await?
                    .ok_or(DomainError::UnknownRole)?;
                User::register_with_role(tenant, email, hash, new.display_name, role.id, role.name)
            }
            None => User::register(tenant, email, hash, new.display_name, new.roles)?,
        };
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
        let new_hash = changes
            .password
            .as_deref()
            .map(|p| {
                User::validate_password(p)?;
                password::hash(p).map_err(|e| UserError::Internal(e.to_string()))
            })
            .transpose()?;
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        // владельцы блокируются до чтения пользователя: параллельные понижения идут по очереди
        let owners = users::lock_active_owners(&mut tx).await?;
        let mut user = users::find_by_id(&mut tx, id)
            .await?
            .ok_or(UserError::NotFound)?;
        let (was_owner, old_roles, old_role_id, old_status) = (
            user.is_active_owner(),
            user.roles.clone(),
            user.role_id,
            user.status,
        );
        match (changes.roles, changes.role_id) {
            (Some(_), Some(_)) => return Err(DomainError::AmbiguousRole.into()),
            (Some(roles), None) => user.change_roles(roles)?,
            (None, Some(role_id)) => {
                let role = roles::find(&mut tx, role_id)
                    .await?
                    .ok_or(DomainError::UnknownRole)?;
                user.assign_custom_role(role.id, role.name);
            }
            (None, None) => {}
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
        if was_owner && !user.is_active_owner() && owners.len() <= 1 {
            return Err(UserError::Conflict(
                "organization must keep an active owner".into(),
            ));
        }
        users::update(&mut tx, &user).await?;
        if let Some(hash) = &new_hash {
            users::set_password_hash(&mut tx, id, hash).await?;
        }
        let access_changed = user.roles != old_roles
            || user.role_id != old_role_id
            || (user.status == UserStatus::Blocked && old_status != UserStatus::Blocked)
            || new_hash.is_some();
        if access_changed {
            refresh_tokens::revoke_all_for_user(&mut tx, id).await?;
        }
        tx.commit().await?;
        Ok(user)
    }
}
