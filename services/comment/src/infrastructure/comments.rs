use chrono::{DateTime, Utc};
use comment_contracts::TargetType;
use platform_core::tenant::TenantId;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::domain::Comment;

#[derive(Debug, Clone, FromRow)]
pub struct CommentRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub author_id: Uuid,
    pub author_name: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
}

impl CommentRow {
    pub fn target(&self) -> TargetType {
        TargetType::from_key(&self.target_type).unwrap_or(TargetType::Person)
    }

    pub fn into_domain(self) -> Comment {
        Comment {
            id: self.id,
            tenant_id: TenantId(self.tenant_id),
            target_type: TargetType::from_key(&self.target_type).unwrap_or(TargetType::Person),
            target_id: self.target_id,
            author_id: self.author_id,
            author_name: self.author_name,
            body: self.body,
        }
    }
}

const SELECT: &str = "SELECT id, tenant_id, target_type, target_id, author_id, author_name, body,
        created_at, edited_at FROM comments";

pub async fn insert(conn: &mut PgConnection, c: &Comment) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO comments (id, tenant_id, target_type, target_id, author_id, author_name, body)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(c.id)
    .bind(c.tenant_id.0)
    .bind(c.target_type.key())
    .bind(c.target_id)
    .bind(c.author_id)
    .bind(&c.author_name)
    .bind(&c.body)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> Result<Option<CommentRow>, sqlx::Error> {
    sqlx::query_as(&format!("{SELECT} WHERE id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// Лента объекта по возрастанию времени; `after` - последняя уже полученная запись.
pub async fn list(
    conn: &mut PgConnection,
    target: TargetType,
    target_id: Uuid,
    after: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<CommentRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{SELECT} WHERE target_type = $1 AND target_id = $2
           AND ($3::timestamptz IS NULL OR (created_at, id) > ($3, $4))
         ORDER BY created_at, id LIMIT $5"
    ))
    .bind(target.key())
    .bind(target_id)
    .bind(after.map(|a| a.0))
    .bind(after.map(|a| a.1))
    .bind(limit)
    .fetch_all(conn)
    .await
}

pub async fn update_body(conn: &mut PgConnection, id: Uuid, body: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE comments SET body = $2, edited_at = clock_timestamp() WHERE id = $1")
        .bind(id)
        .bind(body)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM comments WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}
