//! Проекция заказчиков и контактов из сервиса client и курсор чтения ленты изменений.
use client_contracts::{ClientChange, Entity};
use platform_core::tenant::TenantId;
use sqlx::PgConnection;
use uuid::Uuid;

/// Неблокирующая блокировка синхронизации организации до конца транзакции.
/// `false` - синхронизацию уже ведёт другой запрос.
pub async fn try_lock(conn: &mut PgConnection, tenant: TenantId) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT pg_try_advisory_xact_lock(hashtextextended('clients_sync:' || $1::text, 0))",
    )
    .bind(tenant.0)
    .fetch_one(conn)
    .await
}

/// Позиция в ленте, до которой изменения уже применены.
pub async fn position(conn: &mut PgConnection) -> Result<i64, sqlx::Error> {
    let pos: Option<i64> = sqlx::query_scalar("SELECT position FROM clients_sync")
        .fetch_optional(conn)
        .await?;
    Ok(pos.unwrap_or(0))
}

pub async fn set_position(
    conn: &mut PgConnection,
    tenant: TenantId,
    position: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO clients_sync (tenant_id, position, synced_at) VALUES ($1, $2, now())
         ON CONFLICT (tenant_id) DO UPDATE SET position = EXCLUDED.position, synced_at = now()",
    )
    .bind(tenant.0)
    .bind(position)
    .execute(conn)
    .await?;
    Ok(())
}

/// Применяет записи ленты. Удалённый контакт убирается из проекции; вакансии, которые на него
/// ссылаются, показывают его как отсутствующий.
pub async fn apply(
    conn: &mut PgConnection,
    tenant: TenantId,
    changes: &[ClientChange],
) -> Result<(), sqlx::Error> {
    for change in changes {
        match (change.entity, change.deleted) {
            (Entity::Client, false) => {
                if let Some(s) = &change.client {
                    sqlx::query(
                        "INSERT INTO clients (tenant_id, client_id, name, archived, synced_at)
                         VALUES ($1, $2, $3, $4, now())
                         ON CONFLICT (tenant_id, client_id) DO UPDATE SET
                             name = EXCLUDED.name, archived = EXCLUDED.archived, synced_at = now()",
                    )
                    .bind(tenant.0)
                    .bind(change.entity_id)
                    .bind(&s.name)
                    .bind(s.archived)
                    .execute(&mut *conn)
                    .await?;
                }
            }
            (Entity::Client, true) => {
                sqlx::query("DELETE FROM clients WHERE client_id = $1")
                    .bind(change.entity_id)
                    .execute(&mut *conn)
                    .await?;
            }
            (Entity::Contact, false) => {
                if let Some(s) = &change.contact {
                    sqlx::query(
                        "INSERT INTO client_contacts
                             (tenant_id, contact_id, client_id, full_name, position, synced_at)
                         VALUES ($1, $2, $3, $4, $5, now())
                         ON CONFLICT (tenant_id, contact_id) DO UPDATE SET
                             client_id = EXCLUDED.client_id, full_name = EXCLUDED.full_name,
                             position = EXCLUDED.position, synced_at = now()",
                    )
                    .bind(tenant.0)
                    .bind(change.entity_id)
                    .bind(s.client_id)
                    .bind(&s.full_name)
                    .bind(&s.position)
                    .execute(&mut *conn)
                    .await?;
                }
            }
            (Entity::Contact, true) => {
                sqlx::query("DELETE FROM client_contacts WHERE contact_id = $1")
                    .bind(change.entity_id)
                    .execute(&mut *conn)
                    .await?;
            }
        }
    }
    Ok(())
}

/// Заказчик есть в проекции: `Some(archived)`.
pub async fn archived(
    conn: &mut PgConnection,
    client_id: Uuid,
) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar("SELECT archived FROM clients WHERE client_id = $1")
        .bind(client_id)
        .fetch_optional(conn)
        .await
}

/// Заказчик контакта, если контакт есть в проекции.
pub async fn contact_client(
    conn: &mut PgConnection,
    contact_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT client_id FROM client_contacts WHERE contact_id = $1")
        .bind(contact_id)
        .fetch_optional(conn)
        .await
}
