use platform_auth::Permission;
use platform_core::tenant::TenantId;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::domain::CustomRole;

#[derive(FromRow)]
struct RoleRow {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    permissions: Vec<String>,
}

impl RoleRow {
    fn into_domain(self) -> CustomRole {
        CustomRole {
            id: self.id,
            tenant_id: TenantId(self.tenant_id),
            name: self.name,
            permissions: self
                .permissions
                .iter()
                .filter_map(|p| Permission::parse(p))
                .collect(),
        }
    }
}

const COLUMNS: &str = "id, tenant_id, name, permissions";

fn names(role: &CustomRole) -> Vec<String> {
    role.permissions
        .iter()
        .map(|p| p.as_str().to_string())
        .collect()
}

pub async fn insert(conn: &mut PgConnection, role: &CustomRole) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO roles (id, tenant_id, name, name_key, permissions) VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(role.id)
    .bind(role.tenant_id.0)
    .bind(&role.name)
    .bind(role.key())
    .bind(names(role))
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn find(conn: &mut PgConnection, id: Uuid) -> Result<Option<CustomRole>, sqlx::Error> {
    let row: Option<RoleRow> =
        sqlx::query_as(&format!("SELECT {COLUMNS} FROM roles WHERE id = $1"))
            .bind(id)
            .fetch_optional(conn)
            .await?;
    Ok(row.map(RoleRow::into_domain))
}

/// Собственные роли организации с числом назначенных пользователей, по названию.
pub async fn list_with_counts(
    conn: &mut PgConnection,
) -> Result<Vec<(CustomRole, i64)>, sqlx::Error> {
    #[derive(FromRow)]
    struct Counted {
        #[sqlx(flatten)]
        role: RoleRow,
        users: i64,
    }
    let rows: Vec<Counted> = sqlx::query_as(
        "SELECT r.id, r.tenant_id, r.name, r.permissions,
                (SELECT count(*) FROM users u WHERE u.role_id = r.id) AS users
         FROM roles r ORDER BY r.name_key",
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|c| (c.role.into_domain(), c.users))
        .collect())
}

/// Сколько пользователей организации имеют встроенную роль с этим кодом.
pub async fn built_in_users(conn: &mut PgConnection, code: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM users WHERE $1 = ANY(roles)")
        .bind(code)
        .fetch_one(conn)
        .await
}

pub async fn users_of(conn: &mut PgConnection, id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM users WHERE role_id = $1")
        .bind(id)
        .fetch_one(conn)
        .await
}

pub async fn update(conn: &mut PgConnection, role: &CustomRole) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE roles SET name = $2, name_key = $3, permissions = $4 WHERE id = $1")
        .bind(role.id)
        .bind(&role.name)
        .bind(role.key())
        .bind(names(role))
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM roles WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}
