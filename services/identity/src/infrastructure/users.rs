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
