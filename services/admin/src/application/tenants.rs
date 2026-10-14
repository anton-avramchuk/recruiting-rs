use std::sync::Arc;

use platform_core::tenant::TenantId;
use sqlx::PgPool;

use super::provisioning::{OwnerSpec, Provisioner};
use crate::{
    domain::{DomainError, Slug, Tenant},
    infrastructure::tenants as repo,
};

#[derive(Debug, thiserror::Error)]
pub enum TenantError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("{0}")]
    Invalid(String),
    #[error("slug already taken")]
    SlugTaken,
    #[error("tenant not found")]
    NotFound,
    #[error("could not provision the tenant owner: {0}")]
    Provisioning(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

pub struct NewTenant {
    pub slug: String,
    pub name: String,
    pub owner: OwnerSpec,
}

const MIN_PASSWORD_LEN: usize = 8;

#[derive(Clone)]
pub struct TenantService {
    pool: PgPool,
    provisioner: Arc<dyn Provisioner>,
}

impl TenantService {
    pub fn new(pool: PgPool, provisioner: Arc<dyn Provisioner>) -> Self {
        Self { pool, provisioner }
    }

    /// Сага из двух шагов: запись `provisioning` → владелец в identity → `active`.
    /// При сбое второго шага запись удаляется (компенсация), slug освобождается.
    pub async fn create(&self, new: NewTenant) -> Result<Tenant, TenantError> {
        let slug = Slug::parse(&new.slug)?;
        let mut tenant = Tenant::begin_provisioning(slug, &new.name)?;
        validate_owner(&new.owner)?;
        let owner = OwnerSpec {
            email: new.owner.email.trim().to_lowercase(),
            ..new.owner
        };

        match repo::insert(&self.pool, &tenant).await {
            Ok(()) => {}
            Err(e) if repo::is_unique_violation(&e) => return Err(TenantError::SlugTaken),
            Err(e) => return Err(e.into()),
        }

        if let Err(e) = self.provisioner.provision_owner(tenant.id, &owner).await {
            repo::delete(&self.pool, tenant.id).await?;
            return Err(TenantError::Provisioning(e.to_string()));
        }
        tenant.complete_provisioning()?;
        repo::update_status(&self.pool, &tenant).await?;
        Ok(tenant)
    }

    pub async fn get(&self, id: TenantId) -> Result<Tenant, TenantError> {
        repo::find_by_id(&self.pool, id)
            .await?
            .ok_or(TenantError::NotFound)
    }
}

fn validate_owner(owner: &OwnerSpec) -> Result<(), TenantError> {
    let email = owner.email.trim();
    let ok = email.split_once('@').is_some_and(|(l, d)| {
        !l.is_empty() && d.contains('.') && !email.contains(char::is_whitespace)
    });
    if !ok {
        return Err(TenantError::Invalid("owner email is invalid".into()));
    }
    if owner.password.chars().count() < MIN_PASSWORD_LEN {
        return Err(TenantError::Invalid(format!(
            "owner password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    Ok(())
}
