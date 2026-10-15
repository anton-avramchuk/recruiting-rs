use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::{ClientsSync, Clock, FunnelError};
use crate::{
    domain::{DomainError, NewVacancy, Vacancy, VacancyPatch},
    infrastructure::{
        clients,
        vacancies::{self as store, VacancyView},
    },
};

#[derive(Clone)]
pub struct VacancyService {
    pool: PgPool,
    sync: ClientsSync,
    clock: Clock,
}

/// Заказчик и контакт проверяются по проекции: заказчик существует (и не в архиве, если его
/// выбирают), контакт принадлежит этому заказчику. `fresh == false` - источник недоступен и
/// отсутствие записи ещё ничего не доказывает.
async fn check_refs(
    conn: &mut PgConnection,
    vacancy: &Vacancy,
    before: Option<&Vacancy>,
    fresh: bool,
) -> Result<(), FunnelError> {
    let client_changed = before.is_none_or(|b| b.client_id != vacancy.client_id);
    let contact_changed = before.is_none_or(|b| b.contact_id != vacancy.contact_id);
    if let (true, Some(client_id)) = (client_changed, vacancy.client_id) {
        match clients::archived(conn, client_id).await? {
            Some(false) => {}
            Some(true) => {
                return Err(DomainError::invalid("client_id", "заказчик в архиве").into());
            }
            None if fresh => {
                return Err(DomainError::invalid("client_id", "заказчик не найден").into());
            }
            None => return Err(FunnelError::ClientsUnavailable),
        }
    }
    if let (true, Some(contact_id)) = (contact_changed || client_changed, vacancy.contact_id) {
        match clients::contact_client(conn, contact_id).await? {
            Some(owner) if Some(owner) == vacancy.client_id => {}
            Some(_) => {
                return Err(DomainError::invalid(
                    "contact_id",
                    "контакт не принадлежит выбранному заказчику",
                )
                .into());
            }
            None if fresh => {
                return Err(DomainError::invalid("contact_id", "контакт не найден").into());
            }
            None => return Err(FunnelError::ClientsUnavailable),
        }
    }
    Ok(())
}

impl VacancyService {
    pub fn new(pool: PgPool, sync: ClientsSync, clock: Clock) -> Self {
        Self { pool, sync, clock }
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
        let fresh = self.sync.sync(tenant).await;
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        check_refs(&mut tx, &vacancy, None, fresh).await?;
        store::insert(&mut tx, &vacancy).await?;
        let view = store::get(&mut tx, vacancy.id)
            .await?
            .ok_or(FunnelError::NotFound)?;
        tx.commit().await?;
        Ok(view)
    }

    pub async fn get(&self, tenant: TenantId, id: Uuid) -> Result<VacancyView, FunnelError> {
        self.sync.sync(tenant).await;
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let view = store::get(&mut tx, id).await?;
        tx.rollback().await?;
        view.ok_or(FunnelError::NotFound)
    }

    pub async fn list(
        &self,
        tenant: TenantId,
        client_id: Option<Uuid>,
    ) -> Result<Vec<VacancyView>, FunnelError> {
        self.sync.sync(tenant).await;
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let items = store::list(&mut tx, client_id).await?;
        tx.rollback().await?;
        Ok(items)
    }

    pub async fn update(
        &self,
        tenant: TenantId,
        id: Uuid,
        patch: VacancyPatch,
    ) -> Result<VacancyView, FunnelError> {
        let touches_refs = patch.client_id.is_some() || patch.contact_id.is_some();
        let fresh = if touches_refs {
            self.sync.sync(tenant).await
        } else {
            true
        };
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let current = store::get(&mut tx, id)
            .await?
            .ok_or(FunnelError::NotFound)?;
        let before = current.vacancy.clone();
        let mut vacancy = current.vacancy;
        vacancy.apply(patch, self.clock.today())?;
        check_refs(&mut tx, &vacancy, Some(&before), fresh).await?;
        store::update(&mut tx, &vacancy).await?;
        let view = store::get(&mut tx, id)
            .await?
            .ok_or(FunnelError::NotFound)?;
        tx.commit().await?;
        Ok(view)
    }
}
