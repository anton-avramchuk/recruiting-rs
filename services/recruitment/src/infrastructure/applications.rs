//! Хранение откликов, их истории и интервью. Работает на соединении в контексте тенанта (RLS).
use chrono::{DateTime, Utc};
use platform_core::tenant::TenantId;
use recruitment_contracts::Stage;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::StoreError;
use crate::domain::{Application, DomainError, NewEvent};

#[derive(FromRow)]
struct AppRow {
    id: Uuid,
    tenant_id: Uuid,
    person_id: Uuid,
    vacancy_id: Uuid,
    stage: String,
    stage_changed_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

impl AppRow {
    fn into_domain(self) -> Result<Application, DomainError> {
        let stage = Stage::from_key(&self.stage)
            .ok_or_else(|| DomainError::invalid("stage", "неизвестный этап в базе"))?;
        Ok(Application::restore(
            self.id,
            TenantId(self.tenant_id),
            self.person_id,
            self.vacancy_id,
            stage,
            self.stage_changed_at,
            self.created_at,
        ))
    }
}

const APP_COLUMNS: &str =
    "id, tenant_id, person_id, vacancy_id, stage, stage_changed_at, created_at";

pub async fn insert(conn: &mut PgConnection, a: &Application) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO applications (id, tenant_id, person_id, vacancy_id, stage, stage_changed_at, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(a.id)
    .bind(a.tenant_id.0)
    .bind(a.person_id)
    .bind(a.vacancy_id)
    .bind(a.stage.key())
    .bind(a.stage_changed_at)
    .bind(a.created_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Сохраняет положение в воронке.
pub async fn save_stage(conn: &mut PgConnection, a: &Application) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE applications SET stage = $2, stage_changed_at = $3 WHERE id = $1")
        .bind(a.id)
        .bind(a.stage.key())
        .bind(a.stage_changed_at)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn insert_events(
    conn: &mut PgConnection,
    tenant: TenantId,
    application_id: Uuid,
    events: &[NewEvent],
) -> Result<(), sqlx::Error> {
    for e in events {
        sqlx::query(
            "INSERT INTO application_events (tenant_id, application_id, kind, text, at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(tenant.0)
        .bind(application_id)
        .bind(e.kind.key())
        .bind(&e.text)
        .bind(e.at)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn insert_interview(
    conn: &mut PgConnection,
    tenant: TenantId,
    application_id: Uuid,
    scheduled_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO interviews (tenant_id, application_id, scheduled_at, created_at)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(tenant.0)
    .bind(application_id)
    .bind(scheduled_at)
    .bind(now)
    .execute(conn)
    .await?;
    Ok(())
}

/// Отклик для изменения: строка блокируется до конца транзакции.
pub async fn load_for_update(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<Application>, StoreError> {
    let row: Option<AppRow> = sqlx::query_as(&format!(
        "SELECT {APP_COLUMNS} FROM applications WHERE id = $1 FOR UPDATE"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(AppRow::into_domain).transpose()?)
}

pub async fn load_many_for_update(
    conn: &mut PgConnection,
    ids: &[Uuid],
) -> Result<Vec<Application>, StoreError> {
    let rows: Vec<AppRow> = sqlx::query_as(&format!(
        "SELECT {APP_COLUMNS} FROM applications WHERE id = ANY($1) ORDER BY id FOR UPDATE"
    ))
    .bind(ids)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(AppRow::into_domain)
        .collect::<Result<_, _>>()?)
}

// ---------- чтение со сведениями о человеке и вакансии ----------

/// Строка списка/доски.
#[derive(Debug, FromRow)]
pub struct ApplicationRow {
    pub id: Uuid,
    pub person_id: Uuid,
    pub vacancy_id: Uuid,
    pub vacancy_title: String,
    pub full_name: String,
    pub source: Option<String>,
    pub city: Option<String>,
    pub expected_salary: Option<i64>,
    pub stage: String,
    pub stage_changed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub hours_on_stage: i64,
    pub match_percent: Option<i32>,
    pub person_applications: i64,
    total: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    /// Время на этапе: давно без движения выше.
    Age,
    Name,
    Stage,
    Match,
    Salary,
}

impl Sort {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "age" => Some(Sort::Age),
            "name" => Some(Sort::Name),
            "stage" => Some(Sort::Stage),
            "match" => Some(Sort::Match),
            "salary" => Some(Sort::Salary),
            _ => None,
        }
    }

    fn order_by(self, descending: Option<bool>) -> String {
        let (expr, default_desc, nulls) = match self {
            Sort::Age => ("stage_changed_at", false, ""),
            Sort::Name => ("lower(full_name)", false, ""),
            Sort::Stage => ("stage_order", false, ""),
            Sort::Match => ("match_percent", true, " NULLS LAST"),
            Sort::Salary => ("expected_salary", false, " NULLS LAST"),
        };
        let dir = if descending.unwrap_or(default_desc) {
            "DESC"
        } else {
            "ASC"
        };
        format!("{expr} {dir}{nulls}, id")
    }
}

/// Условия выборки. Пустые поля не ограничивают результат.
#[derive(Debug, Clone, Default)]
pub struct ApplicationFilter {
    pub id: Option<Uuid>,
    pub vacancy_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub stage: Option<Stage>,
    pub source: Option<String>,
    pub q: Option<String>,
}

const ROWS: &str = r"
    SELECT * FROM (
        SELECT a.id, a.person_id, a.vacancy_id, v.title AS vacancy_title,
               p.full_name, p.source, p.city, p.expected_salary,
               a.stage, a.stage_changed_at, a.created_at,
               array_position(ARRAY['new','screening','hr','tech','final','offer','hired','rejected'], a.stage) AS stage_order,
               GREATEST(floor(extract(epoch FROM ($1::timestamptz - a.stage_changed_at)) / 3600), 0)::bigint AS hours_on_stage,
               CASE WHEN cardinality(v.required_skills) = 0 THEN NULL
                    ELSE round(100.0 * (SELECT count(*) FROM unnest(v.required_skills) r
                                        WHERE lower(r) IN (SELECT lower(s) FROM unnest(p.skills) s))
                               / cardinality(v.required_skills))::int
               END AS match_percent,
               (SELECT count(*) FROM applications x WHERE x.person_id = a.person_id) AS person_applications,
               count(*) OVER () AS total
        FROM applications a
        JOIN people p ON p.person_id = a.person_id
        JOIN vacancies v ON v.id = a.vacancy_id
        WHERE ($2::uuid IS NULL OR a.id = $2)
          AND ($3::uuid IS NULL OR a.vacancy_id = $3)
          AND ($4::uuid IS NULL OR a.person_id = $4)
          AND ($5::text IS NULL OR a.stage = $5)
          AND ($6::text IS NULL OR lower(p.source) = lower($6))
          AND ($7::text IS NULL
               OR p.full_name ILIKE $7 ESCAPE '\'
               OR v.title ILIKE $7 ESCAPE '\'
               OR p.city ILIKE $7 ESCAPE '\'
               OR p.email ILIKE $7 ESCAPE '\'
               OR array_to_string(p.skills, ' ') ILIKE $7 ESCAPE '\')
    ) t";

/// Строки списка и общее количество. `now` - момент отсчёта времени на этапе.
pub async fn rows(
    conn: &mut PgConnection,
    filter: &ApplicationFilter,
    now: DateTime<Utc>,
    sort: Sort,
    descending: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ApplicationRow>, i64), sqlx::Error> {
    let q = filter
        .q
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(platform_db::contains_pattern);
    let rows: Vec<ApplicationRow> = sqlx::query_as(&format!(
        "{ROWS} ORDER BY {} LIMIT $8 OFFSET $9",
        sort.order_by(descending)
    ))
    .bind(now)
    .bind(filter.id)
    .bind(filter.vacancy_id)
    .bind(filter.person_id)
    .bind(filter.stage.map(Stage::key))
    .bind(&filter.source)
    .bind(q)
    .bind(limit)
    .bind(offset)
    .fetch_all(conn)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    Ok((rows, total))
}

/// Одна запись по идентификатору.
pub async fn row(
    conn: &mut PgConnection,
    id: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<ApplicationRow>, sqlx::Error> {
    let filter = ApplicationFilter {
        id: Some(id),
        ..ApplicationFilter::default()
    };
    let (mut rows, _) = self::rows(conn, &filter, now, Sort::Age, None, 1, 0).await?;
    Ok(rows.pop())
}

/// Все отклики человека (для панели).
pub async fn person_rows(
    conn: &mut PgConnection,
    person_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<ApplicationRow>, sqlx::Error> {
    let filter = ApplicationFilter {
        person_id: Some(person_id),
        ..ApplicationFilter::default()
    };
    let (rows, _) = self::rows(conn, &filter, now, Sort::Age, Some(true), 200, 0).await?;
    Ok(rows)
}

#[derive(Debug, FromRow)]
pub struct EventRow {
    pub kind: String,
    pub text: String,
    pub at: DateTime<Utc>,
}

/// История отклика: новые записи выше.
pub async fn history(
    conn: &mut PgConnection,
    application_id: Uuid,
) -> Result<Vec<EventRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT kind, text, at FROM application_events WHERE application_id = $1
         ORDER BY at DESC, seq DESC",
    )
    .bind(application_id)
    .fetch_all(conn)
    .await
}
