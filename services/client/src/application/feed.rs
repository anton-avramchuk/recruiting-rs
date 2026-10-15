use client_contracts::{ClientChange, ClientChangesPage, ClientSnapshot, ContactSnapshot, Entity};
use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use sqlx::PgPool;

use super::ClientError;
use crate::infrastructure::changes::{self, ChangeRow};

#[derive(Clone)]
pub struct FeedService {
    pool: PgPool,
}

fn to_change(row: ChangeRow) -> Option<ClientChange> {
    let entity = match row.entity.as_str() {
        "client" => Entity::Client,
        "contact" => Entity::Contact,
        _ => return None,
    };
    let (client, contact) = match (&row.snapshot, entity) {
        (Some(s), Entity::Client) => (
            serde_json::from_value::<ClientSnapshot>(s.clone()).ok(),
            None,
        ),
        (Some(s), Entity::Contact) => (
            None,
            serde_json::from_value::<ContactSnapshot>(s.clone()).ok(),
        ),
        (None, _) => (None, None),
    };
    Some(ClientChange {
        seq: row.seq,
        entity,
        entity_id: row.entity_id,
        deleted: row.deleted,
        client,
        contact,
    })
}

impl FeedService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Изменения организации после позиции `after`, не больше `limit` записей.
    pub async fn changes(
        &self,
        tenant: TenantId,
        after: i64,
        limit: i64,
    ) -> Result<ClientChangesPage, ClientError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let rows = changes::after(&mut tx, after, limit).await?;
        tx.rollback().await?;
        let next = rows.last().map_or(after, |r| r.seq);
        let items = rows.into_iter().filter_map(to_change).collect();
        Ok(ClientChangesPage { items, next })
    }
}
