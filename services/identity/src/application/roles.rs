use platform_auth::Permission;
use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    domain::{BUILT_IN, CustomRole, DomainError},
    infrastructure::{refresh_tokens, roles, users},
};

#[derive(Debug, thiserror::Error)]
pub enum RoleError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("{0}")]
    Conflict(String),
    #[error("role not found")]
    NotFound,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Роль в списке: встроенная или собственная.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleView {
    /// Код встроенной роли либо идентификатор собственной.
    pub id: String,
    pub custom: bool,
    pub name: String,
    pub permissions: Vec<Permission>,
    pub users: i64,
}

#[derive(Default)]
pub struct RoleChanges {
    pub name: Option<String>,
    pub permissions: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct RoleService {
    pool: PgPool,
}

impl RoleService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Встроенные роли, затем собственные по названию.
    pub async fn list(&self, tenant: TenantId) -> Result<Vec<RoleView>, RoleError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let mut out = Vec::new();
        for built_in in &BUILT_IN {
            out.push(RoleView {
                id: built_in.code.to_string(),
                custom: false,
                name: built_in.title.to_string(),
                permissions: built_in.role.permissions().to_vec(),
                users: roles::built_in_users(&mut tx, built_in.code).await?,
            });
        }
        for (role, users) in roles::list_with_counts(&mut tx).await? {
            out.push(RoleView {
                id: role.id.to_string(),
                custom: true,
                name: role.name,
                permissions: role.permissions,
                users,
            });
        }
        tx.rollback().await?;
        Ok(out)
    }

    pub async fn create(
        &self,
        tenant: TenantId,
        name: &str,
        permissions: &[String],
    ) -> Result<RoleView, RoleError> {
        let role = CustomRole::create(tenant, name, permissions)?;
        reject_reserved(&role.name)?;
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        roles::insert(&mut tx, &role).await.map_err(name_taken)?;
        tx.commit().await?;
        Ok(view(role, 0))
    }

    /// Переименование не трогает сессии; смена прав отзывает refresh-токены назначенных пользователей.
    pub async fn update(
        &self,
        tenant: TenantId,
        id: Uuid,
        changes: RoleChanges,
    ) -> Result<RoleView, RoleError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let mut role = roles::find(&mut tx, id).await?.ok_or(RoleError::NotFound)?;
        if let Some(name) = &changes.name {
            role.rename(name)?;
            reject_reserved(&role.name)?;
        }
        let permissions_changed = match &changes.permissions {
            Some(p) => role.set_permissions(p)?,
            None => false,
        };
        roles::update(&mut tx, &role).await.map_err(name_taken)?;
        if permissions_changed {
            refresh_tokens::revoke_all_for_role(&mut tx, id).await?;
        }
        let assigned = roles::users_of(&mut tx, id).await?;
        tx.commit().await?;
        Ok(view(role, assigned))
    }

    pub async fn delete(&self, tenant: TenantId, id: Uuid) -> Result<(), RoleError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        roles::find(&mut tx, id).await?.ok_or(RoleError::NotFound)?;
        let assigned = roles::users_of(&mut tx, id).await?;
        if assigned > 0 {
            return Err(RoleError::Conflict(format!(
                "role is assigned to {assigned} users"
            )));
        }
        roles::delete(&mut tx, id).await?;
        tx.commit().await?;
        Ok(())
    }
}

fn view(role: CustomRole, users: i64) -> RoleView {
    RoleView {
        id: role.id.to_string(),
        custom: true,
        name: role.name,
        permissions: role.permissions,
        users,
    }
}

fn reject_reserved(name: &str) -> Result<(), RoleError> {
    if CustomRole::is_reserved(name) {
        Err(RoleError::Conflict("role name is reserved".into()))
    } else {
        Ok(())
    }
}

fn name_taken(e: sqlx::Error) -> RoleError {
    if users::is_unique_violation(&e) {
        RoleError::Conflict("role name already in use".into())
    } else {
        RoleError::Db(e)
    }
}
