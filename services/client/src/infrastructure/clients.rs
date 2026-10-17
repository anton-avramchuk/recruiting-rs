use chrono::{DateTime, Utc};
use grid_contracts::{GridPage, GridQuery};
use platform_grid::{ColumnType, FetchError, GridColumn, GridSchema, fetch_page};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::domain::Client;

/// Заказчик с числом контактов для списка.
#[derive(Debug, Clone, FromRow)]
pub struct ClientRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub note: Option<String>,
    pub archived: bool,
    pub contacts_count: i64,
    pub created_at: DateTime<Utc>,
}

/// Экранирует `%`, `_` и `\` для LIKE.
fn like_pattern(q: &str) -> String {
    let escaped: String = q
        .chars()
        .flat_map(|c| match c {
            '%' | '_' | '\\' => vec!['\\', c],
            other => vec![other],
        })
        .collect();
    format!("%{escaped}%")
}

const SELECT: &str = "SELECT c.id, c.tenant_id, c.name, c.note, c.archived, c.created_at,
        (SELECT count(*) FROM contacts k WHERE k.client_id = c.id) AS contacts_count
    FROM clients c";

/// Поля списка заказчиков, доступные клиенту (белый список грида).
const GRID_COLUMNS: &[GridColumn] = &[
    GridColumn::new("id", "id", ColumnType::Uuid),
    // сортировка и поиск по нормализованному названию: без учёта регистра и лишних пробелов
    GridColumn::new("name", "name_key", ColumnType::Text),
    GridColumn::new("note", "note", ColumnType::Text),
    GridColumn::new("archived", "archived", ColumnType::Bool),
    GridColumn::new(
        "contacts_count",
        "(SELECT count(*) FROM contacts k WHERE k.client_id = clients.id)",
        ColumnType::Int,
    ),
    GridColumn::new("created_at", "created_at", ColumnType::Timestamp),
];

const GRID: GridSchema = GridSchema {
    source: "clients",
    key: "id",
    columns: GRID_COLUMNS,
    max_page_size: 100,
};

const GRID_SELECT: &str = "id, tenant_id, name, note, archived, created_at,
    (SELECT count(*) FROM contacts k WHERE k.client_id = clients.id) AS contacts_count";

/// Страница заказчиков по запросу грида (изоляция - за RLS соединения).
pub async fn page(
    conn: &mut PgConnection,
    query: &GridQuery,
) -> Result<GridPage<ClientRow>, FetchError> {
    fetch_page(conn, &GRID, GRID_SELECT, query).await
}

pub async fn list(
    conn: &mut PgConnection,
    query: Option<&str>,
    include_archived: bool,
) -> Result<Vec<ClientRow>, sqlx::Error> {
    let pattern = query
        .map(|q| {
            q.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        })
        .filter(|q| !q.is_empty())
        .map(|q| like_pattern(&q));
    sqlx::query_as(&format!(
        "{SELECT} WHERE ($1::text IS NULL OR c.name_key LIKE $1) AND ($2 OR NOT c.archived)
         ORDER BY c.archived, c.name_key"
    ))
    .bind(pattern)
    .bind(include_archived)
    .fetch_all(conn)
    .await
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> Result<Option<ClientRow>, sqlx::Error> {
    sqlx::query_as(&format!("{SELECT} WHERE c.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// Заказчик с таким же названием (по ключу уникальности).
pub async fn find_by_key(
    conn: &mut PgConnection,
    key: &str,
    except: Option<Uuid>,
) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, name FROM clients WHERE name_key = $1 AND ($2::uuid IS NULL OR id <> $2)",
    )
    .bind(key)
    .bind(except)
    .fetch_optional(conn)
    .await
}

pub async fn insert(conn: &mut PgConnection, client: &Client) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO clients (id, tenant_id, name, name_key, note, archived)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(client.id)
    .bind(client.tenant_id.0)
    .bind(&client.name)
    .bind(client.name_key())
    .bind(&client.note)
    .bind(client.archived)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update(conn: &mut PgConnection, client: &Client) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE clients SET name = $2, name_key = $3, note = $4, archived = $5, updated_at = now()
         WHERE id = $1",
    )
    .bind(client.id)
    .bind(&client.name)
    .bind(client.name_key())
    .bind(&client.note)
    .bind(client.archived)
    .execute(conn)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::like_pattern;

    #[test]
    fn like_specials_are_escaped() {
        assert_eq!(like_pattern("a%b_c\\"), "%a\\%b\\_c\\\\%");
        assert_eq!(like_pattern("acme"), "%acme%");
    }
}
