//! Запросы для аналитики. Работают на соединении в контексте тенанта (RLS).
use chrono::{DateTime, NaiveDate, Utc};
use recruitment_contracts::Stage;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::domain::analytics::AppFact;

#[derive(FromRow)]
struct FactRow {
    created_at: DateTime<Utc>,
    stage: String,
    stage_changed_at: DateTime<Utc>,
    source: Option<String>,
    client_id: Option<Uuid>,
    reached: i32,
    first_touch: Option<DateTime<Utc>>,
}

/// Факты об откликах, созданных не раньше `since`: дальше всего пройденный этап по истории
/// и время первого действия.
pub async fn facts(
    conn: &mut PgConnection,
    since: DateTime<Utc>,
) -> Result<Vec<AppFact>, sqlx::Error> {
    let rows: Vec<FactRow> = sqlx::query_as(
        "SELECT a.created_at, a.stage, a.stage_changed_at, p.source, v.client_id,
                COALESCE((SELECT max(array_position(
                              ARRAY['new','screening','hr','tech','final','offer','hired'], e.stage))
                          FROM application_events e
                          WHERE e.application_id = a.id AND e.stage IS NOT NULL), 0)::int AS reached,
                (SELECT min(e.at) FROM application_events e
                 WHERE e.application_id = a.id AND e.kind <> 'created') AS first_touch
         FROM applications a
         JOIN people p ON p.person_id = a.person_id
         JOIN vacancies v ON v.id = a.vacancy_id
         WHERE a.created_at >= $1",
    )
    .bind(since)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            Some(AppFact {
                created_at: r.created_at,
                stage: Stage::from_key(&r.stage)?,
                stage_changed_at: r.stage_changed_at,
                source: r.source,
                client_id: r.client_id,
                reached: usize::try_from(r.reached).unwrap_or(0),
                first_touch: r.first_touch,
            })
        })
        .collect())
}

/// Отклики по неделям (понедельник по UTC) с `since`.
pub async fn week_counts(
    conn: &mut PgConnection,
    since: DateTime<Utc>,
    client: Option<Uuid>,
) -> Result<Vec<(NaiveDate, i64)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT date_trunc('week', a.created_at AT TIME ZONE 'UTC')::date AS week, count(*)
         FROM applications a JOIN vacancies v ON v.id = a.vacancy_id
         WHERE a.created_at >= $1 AND ($2::uuid IS NULL OR v.client_id = $2)
         GROUP BY 1 ORDER BY 1",
    )
    .bind(since)
    .bind(client)
    .fetch_all(conn)
    .await
}

/// Людей с более чем одним откликом (за всё время).
pub async fn repeat_people(
    conn: &mut PgConnection,
    client: Option<Uuid>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM (
             SELECT a.person_id FROM applications a JOIN vacancies v ON v.id = a.vacancy_id
             WHERE $1::uuid IS NULL OR v.client_id = $1
             GROUP BY a.person_id HAVING count(*) > 1) t",
    )
    .bind(client)
    .fetch_one(conn)
    .await
}
