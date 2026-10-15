//! Запросы для аналитики. Работают на соединении в контексте тенанта (RLS).
use chrono::{DateTime, NaiveDate, Utc};
use recruitment_contracts::Stage;
use sqlx::{FromRow, PgConnection};

use crate::domain::analytics::AppFact;

#[derive(FromRow)]
struct FactRow {
    created_at: DateTime<Utc>,
    stage: String,
    stage_changed_at: DateTime<Utc>,
    source: Option<String>,
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
        "SELECT a.created_at, a.stage, a.stage_changed_at, p.source,
                COALESCE((SELECT max(array_position(
                              ARRAY['new','screening','hr','tech','final','offer','hired'], e.stage))
                          FROM application_events e
                          WHERE e.application_id = a.id AND e.stage IS NOT NULL), 0)::int AS reached,
                (SELECT min(e.at) FROM application_events e
                 WHERE e.application_id = a.id AND e.kind <> 'created') AS first_touch
         FROM applications a
         JOIN people p ON p.person_id = a.person_id
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
) -> Result<Vec<(NaiveDate, i64)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT date_trunc('week', created_at AT TIME ZONE 'UTC')::date AS week, count(*)
         FROM applications WHERE created_at >= $1 GROUP BY 1 ORDER BY 1",
    )
    .bind(since)
    .fetch_all(conn)
    .await
}

/// Людей с более чем одним откликом (за всё время).
pub async fn repeat_people(conn: &mut PgConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM (SELECT person_id FROM applications GROUP BY person_id HAVING count(*) > 1) t",
    )
    .fetch_one(conn)
    .await
}
