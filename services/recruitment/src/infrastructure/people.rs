//! Проекция людей из сервиса candidate и курсор чтения ленты изменений.
use candidate_contracts::PersonChange;
use platform_core::tenant::TenantId;
use sqlx::PgConnection;
use uuid::Uuid;

/// Неблокирующая блокировка синхронизации организации до конца транзакции.
/// `false` - синхронизацию уже ведёт другой запрос.
pub async fn try_lock(conn: &mut PgConnection, tenant: TenantId) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT pg_try_advisory_xact_lock(hashtextextended('people_sync:' || $1::text, 0))",
    )
    .bind(tenant.0)
    .fetch_one(conn)
    .await
}

/// Позиция в ленте, до которой изменения уже применены.
pub async fn position(conn: &mut PgConnection) -> Result<i64, sqlx::Error> {
    let pos: Option<i64> = sqlx::query_scalar("SELECT position FROM people_sync")
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
        "INSERT INTO people_sync (tenant_id, position, synced_at) VALUES ($1, $2, now())
         ON CONFLICT (tenant_id) DO UPDATE SET position = EXCLUDED.position, synced_at = now()",
    )
    .bind(tenant.0)
    .bind(position)
    .execute(conn)
    .await?;
    Ok(())
}

/// Применяет записи ленты: обновляет проекцию, а для удалённых людей убирает её строку
/// (отклики уходят каскадом).
pub async fn apply(
    conn: &mut PgConnection,
    tenant: TenantId,
    changes: &[PersonChange],
) -> Result<(), sqlx::Error> {
    for change in changes {
        match (&change.snapshot, change.deleted) {
            (Some(s), false) => {
                sqlx::query(
                    "INSERT INTO people (tenant_id, person_id, full_name, source, city, expected_salary,
                         email, phone, skills, experience_years, synced_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now())
                     ON CONFLICT (tenant_id, person_id) DO UPDATE SET
                         full_name = EXCLUDED.full_name, source = EXCLUDED.source,
                         city = EXCLUDED.city, expected_salary = EXCLUDED.expected_salary,
                         email = EXCLUDED.email, phone = EXCLUDED.phone, skills = EXCLUDED.skills,
                         experience_years = EXCLUDED.experience_years, synced_at = now()",
                )
                .bind(tenant.0)
                .bind(change.person_id)
                .bind(&s.full_name)
                .bind(&s.source)
                .bind(&s.city)
                .bind(s.expected_salary)
                .bind(&s.email)
                .bind(&s.phone)
                .bind(&s.skills)
                .bind(s.experience_years)
                .execute(&mut *conn)
                .await?;
            }
            _ => {
                sqlx::query("DELETE FROM people WHERE person_id = $1")
                    .bind(change.person_id)
                    .execute(&mut *conn)
                    .await?;
            }
        }
    }
    Ok(())
}

pub async fn exists(conn: &mut PgConnection, person_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM people WHERE person_id = $1)")
        .bind(person_id)
        .fetch_one(conn)
        .await
}
