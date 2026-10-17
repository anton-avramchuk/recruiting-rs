use platform_core::tenant::TenantId;
use platform_grid::{
    ColumnType, FetchError, GridColumn, GridSchema,
    contracts::{GridPage, GridQuery},
    fetch_page,
};
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

/// Поля списка организаций, доступные клиенту (белый список грида).
const GRID_COLUMNS: &[GridColumn] = &[
    GridColumn::new("id", "id", ColumnType::Uuid),
    GridColumn::new("slug", "slug", ColumnType::Text),
    GridColumn::new("name", "name", ColumnType::Text),
    GridColumn::new("status", "status", ColumnType::Text),
];

const GRID: GridSchema = GridSchema {
    source: "tenants",
    key: "id",
    columns: GRID_COLUMNS,
    max_page_size: 100,
};

/// Страница организаций по запросу грида. Таблица платформенная, без RLS: доступ проверяет API.
pub async fn page(pool: &PgPool, query: &GridQuery) -> Result<GridPage<Tenant>, FetchError> {
    let mut conn = pool.acquire().await.map_err(FetchError::Db)?;
    let page: GridPage<TenantRow> = fetch_page(&mut conn, &GRID, COLUMNS, query).await?;
    Ok(GridPage {
        items: page
            .items
            .into_iter()
            .filter_map(TenantRow::into_domain)
            .collect(),
        total: page.total,
        page: page.page,
        page_size: page.page_size,
    })
}

const FILTER: &str = r"($1::text IS NULL OR status = $1)
    AND ($2::text IS NULL OR slug ILIKE $2 ESCAPE '\' OR name ILIKE $2 ESCAPE '\')";

pub async fn list(
    pool: &PgPool,
    status: Option<TenantStatus>,
    q: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Tenant>, i64), sqlx::Error> {
    let status = status.map(TenantStatus::as_str);
    let pattern = q
        .filter(|q| !q.is_empty())
        .map(platform_db::contains_pattern);

    let total: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM tenants WHERE {FILTER}"))
        .bind(status)
        .bind(&pattern)
        .fetch_one(pool)
        .await?;
    let rows: Vec<TenantRow> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM tenants WHERE {FILTER} ORDER BY slug LIMIT $3 OFFSET $4"
    ))
    .bind(status)
    .bind(&pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok((
        rows.into_iter()
            .filter_map(TenantRow::into_domain)
            .collect(),
        total,
    ))
}
