//! Синхронизация проекции заказчиков по ленте изменений client (при чтении).
use std::sync::Arc;

use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use sqlx::PgPool;

use super::client_feed::ClientFeed;
use crate::infrastructure::clients;

/// Сколько записей ленты читается за один запрос.
const PAGE: i64 = 500;

#[derive(Clone)]
pub struct ClientsSync {
    pool: PgPool,
    feed: Arc<dyn ClientFeed>,
}

impl ClientsSync {
    pub fn new(pool: PgPool, feed: Arc<dyn ClientFeed>) -> Self {
        Self { pool, feed }
    }

    /// Догоняет проекцию организации. `true` - данные свежие (либо синхронизацию уже ведёт
    /// другой запрос), `false` - источник недоступен и ответ строится по последним
    /// известным данным.
    pub async fn sync(&self, tenant: TenantId) -> bool {
        match self.run(tenant).await {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(error = %e, tenant = %tenant.0, "clients sync skipped");
                false
            }
        }
    }

    async fn run(&self, tenant: TenantId) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        loop {
            let mut tx = TenantTx::begin(&self.pool, tenant).await?;
            if !clients::try_lock(&mut tx, tenant).await? {
                tx.rollback().await?;
                return Ok(());
            }
            let position = clients::position(&mut tx).await?;
            let page = self.feed.changes(tenant, position, PAGE).await?;
            if page.items.is_empty() {
                tx.rollback().await?;
                return Ok(());
            }
            clients::apply(&mut tx, tenant, &page.items).await?;
            clients::set_position(&mut tx, tenant, page.next).await?;
            let full = page.items.len() as i64 >= PAGE;
            tx.commit().await?;
            if !full {
                return Ok(());
            }
        }
    }
}
