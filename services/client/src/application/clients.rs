use grid_contracts::{GridPage, GridQuery, SortKey};
use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::ClientError;
use crate::{
    domain::{
        Client, ClientPatch, Contact, ContactPatch, DomainError, NewClient, NewContact, name_key,
    },
    infrastructure::{
        clients::{self, ClientRow},
        contacts::{self, ContactRow},
        is_unique_violation, positions,
    },
};

/// Карточка заказчика с контактами.
#[derive(Debug, Clone)]
pub struct ClientDetail {
    pub client: ClientRow,
    pub contacts: Vec<ContactRow>,
}

#[derive(Clone)]
pub struct ClientService {
    pool: PgPool,
}

fn domain_client(row: &ClientRow) -> Client {
    Client {
        id: row.id,
        tenant_id: TenantId(row.tenant_id),
        name: row.name.clone(),
        note: row.note.clone(),
        archived: row.archived,
    }
}

/// Должность контакта должна существовать в справочнике организации.
async fn ensure_position(
    conn: &mut PgConnection,
    position_id: Option<Uuid>,
) -> Result<(), ClientError> {
    if let Some(id) = position_id
        && positions::get(conn, id).await?.is_none()
    {
        return Err(
            DomainError::invalid("position_id", "такой должности нет в справочнике").into(),
        );
    }
    Ok(())
}

impl ClientService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, tenant: TenantId, new: NewClient) -> Result<ClientRow, ClientError> {
        let client = Client::create(tenant, new)?;
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        if let Some((id, name)) = clients::find_by_key(&mut tx, &client.name_key(), None).await? {
            return Err(ClientError::DuplicateClient { id, name });
        }
        if let Err(e) = clients::insert(&mut tx, &client).await {
            if is_unique_violation(&e) {
                return Err(self.duplicate_after_race(tenant, &client).await);
            }
            return Err(e.into());
        }
        let row = clients::get(&mut tx, client.id)
            .await?
            .ok_or(ClientError::NotFound)?;
        tx.commit().await?;
        Ok(row)
    }

    /// Параллельное создание проиграло гонку: найти победителя и сообщить о дубле.
    async fn duplicate_after_race(&self, tenant: TenantId, client: &Client) -> ClientError {
        let found = async {
            let mut tx = TenantTx::begin(&self.pool, tenant).await?;
            let found = clients::find_by_key(&mut tx, &name_key(&client.name), None).await?;
            tx.rollback().await?;
            Ok::<_, sqlx::Error>(found)
        }
        .await;
        match found {
            Ok(Some((id, name))) => ClientError::DuplicateClient { id, name },
            Ok(None) => ClientError::NotFound,
            Err(e) => e.into(),
        }
    }

    pub async fn list(
        &self,
        tenant: TenantId,
        query: Option<&str>,
        include_archived: bool,
    ) -> Result<Vec<ClientRow>, ClientError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let rows = clients::list(&mut tx, query, include_archived).await?;
        tx.rollback().await?;
        Ok(rows)
    }

    /// Страница заказчиков; без сортировки - активные по названию, архивные в конце.
    pub async fn page(
        &self,
        tenant: TenantId,
        mut query: GridQuery,
    ) -> Result<GridPage<ClientRow>, ClientError> {
        if query.sort.is_empty() {
            query.sort.push(SortKey::asc("archived"));
            query.sort.push(SortKey::asc("name"));
        }
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let page = clients::page(&mut tx, &query).await;
        tx.rollback().await?;
        Ok(page?)
    }

    pub async fn get(&self, tenant: TenantId, id: Uuid) -> Result<ClientDetail, ClientError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let client = clients::get(&mut tx, id)
            .await?
            .ok_or(ClientError::NotFound)?;
        let contacts = contacts::list_for_client(&mut tx, id).await?;
        tx.rollback().await?;
        Ok(ClientDetail { client, contacts })
    }

    pub async fn update(
        &self,
        tenant: TenantId,
        id: Uuid,
        patch: ClientPatch,
    ) -> Result<ClientRow, ClientError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let row = clients::get(&mut tx, id)
            .await?
            .ok_or(ClientError::NotFound)?;
        let mut client = domain_client(&row);
        client.apply(patch)?;
        if let Some((other, name)) =
            clients::find_by_key(&mut tx, &client.name_key(), Some(id)).await?
        {
            return Err(ClientError::DuplicateClient { id: other, name });
        }
        clients::update(&mut tx, &client).await?;
        let updated = clients::get(&mut tx, id)
            .await?
            .ok_or(ClientError::NotFound)?;
        tx.commit().await?;
        Ok(updated)
    }

    pub async fn add_contact(
        &self,
        tenant: TenantId,
        client_id: Uuid,
        new: NewContact,
    ) -> Result<ContactRow, ClientError> {
        let contact = Contact::create(tenant, client_id, new)?;
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        clients::get(&mut tx, client_id)
            .await?
            .ok_or(ClientError::NotFound)?;
        ensure_position(&mut tx, contact.position_id).await?;
        contacts::insert(&mut tx, &contact).await?;
        let row = contacts::get(&mut tx, contact.id)
            .await?
            .ok_or(ClientError::NotFound)?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn update_contact(
        &self,
        tenant: TenantId,
        id: Uuid,
        patch: ContactPatch,
    ) -> Result<ContactRow, ClientError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let mut contact = contacts::get(&mut tx, id)
            .await?
            .ok_or(ClientError::NotFound)?
            .into_domain();
        contact.apply(patch)?;
        ensure_position(&mut tx, contact.position_id).await?;
        contacts::update(&mut tx, &contact).await?;
        let row = contacts::get(&mut tx, id)
            .await?
            .ok_or(ClientError::NotFound)?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn delete_contact(&self, tenant: TenantId, id: Uuid) -> Result<(), ClientError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        if !contacts::delete(&mut tx, id).await? {
            return Err(ClientError::NotFound);
        }
        tx.commit().await?;
        Ok(())
    }
}
