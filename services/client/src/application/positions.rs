use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use sqlx::PgPool;
use uuid::Uuid;

use super::ClientError;
use crate::{
    domain::Position,
    infrastructure::{
        is_foreign_key_violation, is_unique_violation,
        positions::{self, PositionRow},
    },
};

#[derive(Clone)]
pub struct PositionService {
    pool: PgPool,
}

impl PositionService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(&self, tenant: TenantId) -> Result<Vec<PositionRow>, ClientError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let rows = positions::list(&mut tx).await?;
        tx.rollback().await?;
        Ok(rows)
    }

    pub async fn create(&self, tenant: TenantId, name: &str) -> Result<PositionRow, ClientError> {
        let position = Position::create(tenant, name)?;
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        if let Some(existing) = positions::find_by_key(&mut tx, &position.name_key(), None).await? {
            return Err(ClientError::DuplicatePosition {
                id: existing.id,
                name: existing.name,
            });
        }
        if let Err(e) = positions::insert(&mut tx, &position).await {
            if is_unique_violation(&e) {
                return Err(ClientError::DuplicatePosition {
                    id: position.id,
                    name: position.name,
                });
            }
            return Err(e.into());
        }
        tx.commit().await?;
        Ok(PositionRow {
            id: position.id,
            tenant_id: tenant.0,
            name: position.name,
        })
    }

    pub async fn rename(
        &self,
        tenant: TenantId,
        id: Uuid,
        name: &str,
    ) -> Result<PositionRow, ClientError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let mut position = positions::get(&mut tx, id)
            .await?
            .ok_or(ClientError::NotFound)?
            .into_domain();
        position.rename(name)?;
        if let Some(existing) =
            positions::find_by_key(&mut tx, &position.name_key(), Some(id)).await?
        {
            return Err(ClientError::DuplicatePosition {
                id: existing.id,
                name: existing.name,
            });
        }
        positions::update(&mut tx, &position).await?;
        tx.commit().await?;
        Ok(PositionRow {
            id,
            tenant_id: tenant.0,
            name: position.name,
        })
    }

    pub async fn delete(&self, tenant: TenantId, id: Uuid) -> Result<(), ClientError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        positions::get(&mut tx, id)
            .await?
            .ok_or(ClientError::NotFound)?;
        let used = positions::usage(&mut tx, id).await?;
        if used > 0 {
            return Err(ClientError::PositionInUse(used));
        }
        match positions::delete(&mut tx, id).await {
            Ok(_) => {}
            Err(e) if is_foreign_key_violation(&e) => return Err(ClientError::PositionInUse(1)),
            Err(e) => return Err(e.into()),
        }
        tx.commit().await?;
        Ok(())
    }
}
