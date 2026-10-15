use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::domain::Position;

#[derive(Debug, Clone, FromRow)]
pub struct PositionRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
}

impl PositionRow {
    pub fn into_domain(self) -> Position {
        Position {
            id: self.id,
            tenant_id: platform_core::tenant::TenantId(self.tenant_id),
            name: self.name,
        }
    }
}

pub async fn list(conn: &mut PgConnection) -> Result<Vec<PositionRow>, sqlx::Error> {
    sqlx::query_as("SELECT id, tenant_id, name FROM positions ORDER BY name_key")
        .fetch_all(conn)
        .await
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> Result<Option<PositionRow>, sqlx::Error> {
    sqlx::query_as("SELECT id, tenant_id, name FROM positions WHERE id = $1")
        .bind(id)
        .fetch_optional(conn)
        .await
}

pub async fn find_by_key(
    conn: &mut PgConnection,
    key: &str,
    except: Option<Uuid>,
) -> Result<Option<PositionRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, tenant_id, name FROM positions
         WHERE name_key = $1 AND ($2::uuid IS NULL OR id <> $2)",
    )
    .bind(key)
    .bind(except)
    .fetch_optional(conn)
    .await
}

pub async fn insert(conn: &mut PgConnection, position: &Position) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO positions (id, tenant_id, name, name_key) VALUES ($1, $2, $3, $4)")
        .bind(position.id)
        .bind(position.tenant_id.0)
        .bind(&position.name)
        .bind(position.name_key())
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn update(conn: &mut PgConnection, position: &Position) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE positions SET name = $2, name_key = $3 WHERE id = $1")
        .bind(position.id)
        .bind(&position.name)
        .bind(position.name_key())
        .execute(conn)
        .await?;
    Ok(())
}

/// Сколько контактов используют должность.
pub async fn usage(conn: &mut PgConnection, id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM contacts WHERE position_id = $1")
        .bind(id)
        .fetch_one(conn)
        .await
}

pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM positions WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() > 0)
}
