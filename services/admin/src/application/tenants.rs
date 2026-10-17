use std::sync::Arc;

use platform_core::tenant::TenantId;
use platform_grid::contracts::{GridPage, GridQuery, SortKey};
use sqlx::PgPool;

use super::provisioning::{OwnerSpec, Provisioner};
use crate::{
    domain::{DomainError, Slug, Tenant, TenantStatus},
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
    Grid(#[from] platform_grid::GridError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl From<platform_grid::FetchError> for TenantError {
    fn from(e: platform_grid::FetchError) -> Self {
        match e {
            platform_grid::FetchError::Grid(e) => TenantError::Grid(e),
            platform_grid::FetchError::Db(e) => TenantError::Db(e),
        }
    }
}

pub struct NewTenant {
    pub slug: String,
    pub name: String,
    pub owner: OwnerSpec,
}

const MIN_PASSWORD_LEN: usize = 8;
pub const DEFAULT_PAGE_SIZE: i64 = 20;
pub const MAX_PAGE_SIZE: i64 = 100;

pub struct ListQuery {
    pub status: Option<TenantStatus>,
    pub q: Option<String>,
    pub page: i64,
    pub page_size: i64,
}

pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

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

    pub async fn suspend(&self, id: TenantId) -> Result<Tenant, TenantError> {
        let mut tenant = self.get(id).await?;
        tenant.suspend()?;
        repo::update_status(&self.pool, &tenant).await?;
        Ok(tenant)
    }

    pub async fn activate(&self, id: TenantId) -> Result<Tenant, TenantError> {
        let mut tenant = self.get(id).await?;
        tenant.activate()?;
        repo::update_status(&self.pool, &tenant).await?;
        Ok(tenant)
    }

    /// Страница организаций по запросу грида; без сортировки - по slug.
    pub async fn list_grid(&self, mut query: GridQuery) -> Result<GridPage<Tenant>, TenantError> {
        if query.sort.is_empty() {
            query.sort.push(SortKey::asc("slug"));
        }
        Ok(repo::page(&self.pool, &query).await?)
    }

    pub async fn list(&self, query: ListQuery) -> Result<Page<Tenant>, TenantError> {
        if query.page < 1 || query.page_size < 1 {
            return Err(TenantError::Invalid(
                "page and page_size must be positive".into(),
            ));
        }
        let page_size = query.page_size.min(MAX_PAGE_SIZE);
        let offset = (query.page - 1).saturating_mul(page_size);
        let (items, total) = repo::list(
            &self.pool,
            query.status,
            query.q.as_deref().map(str::trim),
            page_size,
            offset,
        )
        .await?;
        Ok(Page {
            items,
            total,
            page: query.page,
            page_size,
        })
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
