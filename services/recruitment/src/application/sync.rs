//! Синхронизация проекции людей по ленте изменений candidate (при чтении).
use std::sync::Arc;

use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use sqlx::PgPool;

use super::feed::PersonFeed;
use crate::infrastructure::people;

/// Сколько записей ленты читается за один запрос.
const PAGE: i64 = 500;

#[derive(Clone)]
pub struct PeopleSync {
    pool: PgPool,
    feed: Arc<dyn PersonFeed>,
}

impl PeopleSync {
    pub fn new(pool: PgPool, feed: Arc<dyn PersonFeed>) -> Self {
        Self { pool, feed }
    }

    /// Догоняет проекцию организации. `true` - данные свежие (либо синхронизацию уже ведёт
    /// другой запрос), `false` - источник недоступен и воронка отвечает по последним
    /// известным данным.
    pub async fn sync(&self, tenant: TenantId) -> bool {
        match self.run(tenant).await {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(error = %e, tenant = %tenant.0, "people sync skipped");
                false
            }
        }
    }

    async fn run(&self, tenant: TenantId) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        loop {
            let mut tx = TenantTx::begin(&self.pool, tenant).await?;
            if !people::try_lock(&mut tx, tenant).await? {
                tx.rollback().await?;
                return Ok(());
            }
            let position = people::position(&mut tx).await?;
            let page = self.feed.changes(tenant, position, PAGE).await?;
            if page.items.is_empty() {
                tx.rollback().await?;
                return Ok(());
            }
            people::apply(&mut tx, tenant, &page.items).await?;
            people::set_position(&mut tx, tenant, page.next).await?;
            let full = page.items.len() as i64 >= PAGE;
            tx.commit().await?;
            if !full {
                return Ok(());
            }
        }
    }
}
