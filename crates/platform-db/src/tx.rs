use std::ops::{Deref, DerefMut};

use platform_core::tenant::TenantId;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};

/// Транзакция в контексте тенанта.
///
/// Контекст задаётся через `set_config('app.tenant_id', .., true)` (аналог `SET LOCAL`),
/// поэтому живёт только до конца транзакции и не просачивается через пул.
/// Политики RLS читают его через `NULLIF(current_setting('app.tenant_id', true), '')::uuid`.
pub struct TenantTx<'c> {
    tx: Transaction<'c, Postgres>,
    tenant: TenantId,
}

impl<'c> TenantTx<'c> {
    pub async fn begin(pool: &'c PgPool, tenant: TenantId) -> Result<Self, sqlx::Error> {
        let mut tx = pool.begin().await?;
        sqlx::query("SELECT set_config('app.tenant_id', $1, true)")
            .bind(tenant.to_string())
            .execute(&mut *tx)
            .await?;
        Ok(Self { tx, tenant })
    }

    pub fn tenant(&self) -> TenantId {
        self.tenant
    }

    pub async fn commit(self) -> Result<(), sqlx::Error> {
        self.tx.commit().await
    }

    pub async fn rollback(self) -> Result<(), sqlx::Error> {
        self.tx.rollback().await
    }
}

impl Deref for TenantTx<'_> {
    type Target = PgConnection;
    fn deref(&self) -> &PgConnection {
        &self.tx
    }
}

impl DerefMut for TenantTx<'_> {
    fn deref_mut(&mut self) -> &mut PgConnection {
        &mut self.tx
    }
}
