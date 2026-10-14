use platform_core::tenant::TenantId;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::domain::{Slug, Tenant, TenantStatus};

#[derive(FromRow)]
struct TenantRow {
    id: Uuid,
    slug: String,
    name: String,
    status: String,
}

impl TenantRow {
    fn into_domain(self) -> Option<Tenant> {
        Some(Tenant {
            id: TenantId(self.id),
            slug: Slug::parse(&self.slug).ok()?,
            name: self.name,
            status: TenantStatus::parse(&self.status)?,
        })
    }
}

const COLUMNS: &str = "id, slug, name, status";

pub async fn insert(pool: &PgPool, t: &Tenant) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO tenants (id, slug, name, status) VALUES ($1, $2, $3, $4)")
        .bind(t.id.0)
        .bind(t.slug.as_str())
        .bind(&t.name)
        .bind(t.status.as_str())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &PgPool, id: TenantId) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(id.0)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_status(pool: &PgPool, t: &Tenant) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE tenants SET status = $2 WHERE id = $1")
        .bind(t.id.0)
        .bind(t.status.as_str())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn find_by_id(pool: &PgPool, id: TenantId) -> Result<Option<Tenant>, sqlx::Error> {
    let row: Option<TenantRow> =
        sqlx::query_as(&format!("SELECT {COLUMNS} FROM tenants WHERE id = $1"))
            .bind(id.0)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(TenantRow::into_domain))
}

pub async fn find_by_slug(pool: &PgPool, slug: &str) -> Result<Option<Tenant>, sqlx::Error> {
    let row: Option<TenantRow> =
        sqlx::query_as(&format!("SELECT {COLUMNS} FROM tenants WHERE slug = $1"))
            .bind(slug)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(TenantRow::into_domain))
}

pub fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23505")
}
