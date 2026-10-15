//! Чтение ленты изменений (запись делают триггеры БД в транзакции изменения).
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct ChangeRow {
    pub seq: i64,
    pub entity: String,
    pub entity_id: Uuid,
    pub deleted: bool,
    pub snapshot: Option<serde_json::Value>,
}

pub async fn after(
    conn: &mut PgConnection,
    position: i64,
    limit: i64,
) -> Result<Vec<ChangeRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT seq, entity, entity_id, deleted, snapshot FROM client_changes
         WHERE seq > $1 ORDER BY seq LIMIT $2",
    )
    .bind(position)
    .bind(limit)
    .fetch_all(conn)
    .await
}
