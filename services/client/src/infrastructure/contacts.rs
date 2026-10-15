use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::domain::Contact;

/// Контакт с названием должности.
#[derive(Debug, Clone, FromRow)]
pub struct ContactRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub client_id: Uuid,
    pub full_name: String,
    pub position_id: Option<Uuid>,
    pub position_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub note: Option<String>,
}

impl ContactRow {
    pub fn into_domain(self) -> Contact {
        Contact {
            id: self.id,
            tenant_id: platform_core::tenant::TenantId(self.tenant_id),
            client_id: self.client_id,
            full_name: self.full_name,
            position_id: self.position_id,
            email: self.email,
            phone: self.phone,
            note: self.note,
        }
    }
}

const SELECT: &str = "SELECT k.id, k.tenant_id, k.client_id, k.full_name, k.position_id,
        p.name AS position_name, k.email, k.phone, k.note
    FROM contacts k LEFT JOIN positions p ON p.tenant_id = k.tenant_id AND p.id = k.position_id";

pub async fn list_for_client(
    conn: &mut PgConnection,
    client_id: Uuid,
) -> Result<Vec<ContactRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{SELECT} WHERE k.client_id = $1 ORDER BY k.created_at, k.id"
    ))
    .bind(client_id)
    .fetch_all(conn)
    .await
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> Result<Option<ContactRow>, sqlx::Error> {
    sqlx::query_as(&format!("{SELECT} WHERE k.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

pub async fn insert(conn: &mut PgConnection, contact: &Contact) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO contacts (id, tenant_id, client_id, full_name, position_id, email, phone, note)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(contact.id)
    .bind(contact.tenant_id.0)
    .bind(contact.client_id)
    .bind(&contact.full_name)
    .bind(contact.position_id)
    .bind(&contact.email)
    .bind(&contact.phone)
    .bind(&contact.note)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update(conn: &mut PgConnection, contact: &Contact) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE contacts SET full_name = $2, position_id = $3, email = $4, phone = $5, note = $6,
                updated_at = now()
         WHERE id = $1",
    )
    .bind(contact.id)
    .bind(&contact.full_name)
    .bind(contact.position_id)
    .bind(&contact.email)
    .bind(&contact.phone)
    .bind(&contact.note)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM contacts WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() > 0)
}
