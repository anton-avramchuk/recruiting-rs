use grid_contracts::{GridPage, GridQuery};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use platform_grid::{ColumnType, FetchError, GridColumn, GridSchema, fetch_page};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::domain::{Email, User, UserStatus};

#[derive(FromRow)]
struct UserRow {
    id: Uuid,
    tenant_id: Uuid,
    email: String,
    password_hash: String,
    display_name: String,
    roles: Vec<String>,
    role_id: Option<Uuid>,
    role_name: Option<String>,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl UserRow {
    fn into_domain(self) -> Option<User> {
        Some(User {
            id: self.id,
            tenant_id: TenantId(self.tenant_id),
            email: Email::parse(&self.email).ok()?,
            password_hash: self.password_hash,
            display_name: self.display_name,
            roles: self.roles.iter().filter_map(|r| Role::parse(r)).collect(),
            role_id: self.role_id,
            role_name: self.role_name,
            status: UserStatus::parse(&self.status)?,
            created_at: Some(self.created_at),
        })
    }
}

const COLUMNS: &str = "id, tenant_id, email, password_hash, display_name, roles, role_id, \
     (SELECT r.name FROM roles r WHERE r.id = users.role_id) AS role_name, status, created_at";

/// Читает в контексте тенанта соединения (RLS), поэтому фильтр по тенанту не нужен.
pub async fn find_by_email(
    conn: &mut PgConnection,
    email: &Email,
) -> Result<Option<User>, sqlx::Error> {
    let row: Option<UserRow> =
        sqlx::query_as(&format!("SELECT {COLUMNS} FROM users WHERE email = $1"))
            .bind(email.as_str())
            .fetch_optional(conn)
            .await?;
    Ok(row.and_then(UserRow::into_domain))
}

pub async fn find_by_id(conn: &mut PgConnection, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    let row: Option<UserRow> =
        sqlx::query_as(&format!("SELECT {COLUMNS} FROM users WHERE id = $1"))
            .bind(id)
            .fetch_optional(conn)
            .await?;
    Ok(row.and_then(UserRow::into_domain))
}

/// Поля списка пользователей, доступные клиенту (белый список грида). Хеш пароля в схему не входит.
const GRID_COLUMNS: &[GridColumn] = &[
    GridColumn::new("id", "id", ColumnType::Uuid),
    GridColumn::new("email", "email", ColumnType::Text),
    GridColumn::new("display_name", "display_name", ColumnType::Text),
    GridColumn::new("status", "status", ColumnType::Text),
    // встроенные роли через запятую либо идентификатор собственной
    GridColumn::new(
        "role",
        "COALESCE(NULLIF(array_to_string(roles, ','), ''), role_id::text)",
        ColumnType::Text,
    )
    .no_sort(),
    GridColumn::new("created_at", "created_at", ColumnType::Timestamp),
];

const GRID: GridSchema = GridSchema {
    source: "users",
    key: "id",
    columns: GRID_COLUMNS,
    max_page_size: 100,
};

/// Страница пользователей тенанта (изоляция - за RLS соединения).
pub async fn page(
    conn: &mut PgConnection,
    query: &GridQuery,
) -> Result<GridPage<User>, FetchError> {
    let page: GridPage<UserRow> = fetch_page(conn, &GRID, COLUMNS, query).await?;
    Ok(GridPage {
        items: page
            .items
            .into_iter()
            .filter_map(UserRow::into_domain)
            .collect(),
        total: page.total,
        page: page.page,
        page_size: page.page_size,
    })
}

pub async fn insert(conn: &mut PgConnection, user: &User) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, display_name, roles, role_id, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(user.id)
    .bind(user.tenant_id.0)
    .bind(user.email.as_str())
    .bind(&user.password_hash)
    .bind(&user.display_name)
    .bind(role_names(user))
    .bind(user.role_id)
    .bind(user.status.as_str())
    .execute(conn)
    .await?;
    Ok(())
}

/// Сохраняет изменяемые поля (роли, статус, имя).
pub async fn update(conn: &mut PgConnection, user: &User) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE users SET display_name = $2, roles = $3, role_id = $4, status = $5 WHERE id = $1",
    )
    .bind(user.id)
    .bind(&user.display_name)
    .bind(role_names(user))
    .bind(user.role_id)
    .bind(user.status.as_str())
    .execute(conn)
    .await?;
    Ok(())
}

/// Блокирует строки активных владельцев тенанта (в порядке `id`, чтобы параллельные изменения
/// шли по очереди) и возвращает их идентификаторы.
pub async fn lock_active_owners(conn: &mut PgConnection) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM users WHERE 'owner' = ANY(roles) AND status = 'active' ORDER BY id FOR UPDATE",
    )
    .fetch_all(conn)
    .await
}

/// Сохраняет новый хеш пароля.
pub async fn set_password_hash(
    conn: &mut PgConnection,
    id: Uuid,
    hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
        .bind(id)
        .bind(hash)
        .execute(conn)
        .await?;
    Ok(())
}

fn role_names(user: &User) -> Vec<String> {
    user.roles.iter().map(|r| r.as_str().to_string()).collect()
}

pub fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23505")
}
