use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use sqlx::PgPool;
use uuid::Uuid;

use super::{Clock, FunnelError};
use crate::{
    domain::{NewVacancy, Vacancy, VacancyPatch},
    infrastructure::vacancies::{self as store, VacancyView},
};

#[derive(Clone)]
pub struct VacancyService {
    pool: PgPool,
    clock: Clock,
}

impl VacancyService {
    pub fn new(pool: PgPool, clock: Clock) -> Self {
        Self { pool, clock }
    }

    pub fn clock(&self) -> Clock {
        self.clock
    }

    pub async fn create(
        &self,
        tenant: TenantId,
        new: NewVacancy,
    ) -> Result<VacancyView, FunnelError> {
        let vacancy = Vacancy::create(tenant, new, self.clock.today())?;
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        store::insert(&mut tx, &vacancy).await?;
        tx.commit().await?;
        Ok(VacancyView {
            vacancy,
            active_applications: 0,
            total_applications: 0,
        })
    }

    pub async fn get(&self, tenant: TenantId, id: Uuid) -> Result<VacancyView, FunnelError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let view = store::get(&mut tx, id).await?;
        tx.rollback().await?;
        view.ok_or(FunnelError::NotFound)
    }

    pub async fn list(&self, tenant: TenantId) -> Result<Vec<VacancyView>, FunnelError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let items = store::list(&mut tx).await?;
        tx.rollback().await?;
        Ok(items)
    }

    pub async fn update(
        &self,
        tenant: TenantId,
        id: Uuid,
        patch: VacancyPatch,
    ) -> Result<VacancyView, FunnelError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let mut view = store::get(&mut tx, id)
            .await?
            .ok_or(FunnelError::NotFound)?;
        view.vacancy.apply(patch, self.clock.today())?;
        store::update(&mut tx, &view.vacancy).await?;
        tx.commit().await?;
        Ok(view)
    }
}
