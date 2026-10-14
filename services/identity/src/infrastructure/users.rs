use platform_auth::Role;
use platform_core::tenant::TenantId;
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
    status: String,
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
            status: UserStatus::parse(&self.status)?,
        })
    }
}

const COLUMNS: &str = "id, tenant_id, email, password_hash, display_name, roles, status";

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

pub async fn list(conn: &mut PgConnection) -> Result<Vec<User>, sqlx::Error> {
    let rows: Vec<UserRow> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM users ORDER BY email"))
        .fetch_all(conn)
        .await?;
    Ok(rows.into_iter().filter_map(UserRow::into_domain).collect())
}

pub async fn insert(conn: &mut PgConnection, user: &User) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, display_name, roles, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(user.id)
    .bind(user.tenant_id.0)
    .bind(user.email.as_str())
    .bind(&user.password_hash)
    .bind(&user.display_name)
    .bind(role_names(user))
    .bind(user.status.as_str())
    .execute(conn)
    .await?;
    Ok(())
}

/// Сохраняет изменяемые поля (роли, статус, имя).
pub async fn update(conn: &mut PgConnection, user: &User) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET display_name = $2, roles = $3, status = $4 WHERE id = $1")
        .bind(user.id)
        .bind(&user.display_name)
        .bind(role_names(user))
        .bind(user.status.as_str())
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
