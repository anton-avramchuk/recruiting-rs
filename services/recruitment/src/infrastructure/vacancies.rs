//! Хранение вакансий. Работает на соединении в контексте тенанта (RLS).
use chrono::NaiveDate;
use platform_core::tenant::TenantId;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::StoreError;
use crate::domain::Vacancy;

#[derive(FromRow)]
struct VacancyRow {
    id: Uuid,
    tenant_id: Uuid,
    title: String,
    department: Option<String>,
    city: Option<String>,
    openings: i32,
    priority: String,
    hiring_manager: Option<String>,
    recruiter: Option<String>,
    required_skills: Vec<String>,
    opened_on: NaiveDate,
    active_applications: i64,
    total_applications: i64,
}

/// Вакансия со счётчиками откликов.
pub struct VacancyView {
    pub vacancy: Vacancy,
    pub active_applications: i64,
    pub total_applications: i64,
}

const SELECT: &str = "
    SELECT v.id, v.tenant_id, v.title, v.department, v.city, v.openings, v.priority,
           v.hiring_manager, v.recruiter, v.required_skills, v.opened_on,
           count(a.id) FILTER (WHERE a.stage NOT IN ('hired', 'rejected')) AS active_applications,
           count(a.id) AS total_applications
    FROM vacancies v LEFT JOIN applications a ON a.vacancy_id = v.id";

fn view(row: VacancyRow) -> Result<VacancyView, StoreError> {
    Ok(VacancyView {
        vacancy: Vacancy::restore(
            row.id,
            TenantId(row.tenant_id),
            row.title,
            row.department,
            row.city,
            row.openings,
            &row.priority,
            row.hiring_manager,
            row.recruiter,
            row.required_skills,
            row.opened_on,
        )?,
        active_applications: row.active_applications,
        total_applications: row.total_applications,
    })
}

pub async fn insert(conn: &mut PgConnection, v: &Vacancy) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO vacancies (id, tenant_id, title, department, city, openings, priority,
             hiring_manager, recruiter, required_skills, opened_on)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(v.id)
    .bind(v.tenant_id.0)
    .bind(&v.title)
    .bind(&v.department)
    .bind(&v.city)
    .bind(v.openings)
    .bind(v.priority.key())
    .bind(&v.hiring_manager)
    .bind(&v.recruiter)
    .bind(&v.required_skills)
    .bind(v.opened_on)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update(conn: &mut PgConnection, v: &Vacancy) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE vacancies SET title = $2, department = $3, city = $4, openings = $5, priority = $6,
             hiring_manager = $7, recruiter = $8, required_skills = $9, opened_on = $10,
             updated_at = now()
         WHERE id = $1",
    )
    .bind(v.id)
    .bind(&v.title)
    .bind(&v.department)
    .bind(&v.city)
    .bind(v.openings)
    .bind(v.priority.key())
    .bind(&v.hiring_manager)
    .bind(&v.recruiter)
    .bind(&v.required_skills)
    .bind(v.opened_on)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> Result<Option<VacancyView>, StoreError> {
    let row: Option<VacancyRow> =
        sqlx::query_as(&format!("{SELECT} WHERE v.id = $1 GROUP BY v.id"))
            .bind(id)
            .fetch_optional(conn)
            .await?;
    row.map(view).transpose()
}

/// Приоритетные выше, затем давно открытые.
pub async fn list(conn: &mut PgConnection) -> Result<Vec<VacancyView>, StoreError> {
    let rows: Vec<VacancyRow> = sqlx::query_as(&format!(
        "{SELECT} GROUP BY v.id
         ORDER BY CASE v.priority WHEN 'high' THEN 0 WHEN 'normal' THEN 1 ELSE 2 END,
                  v.opened_on, v.title, v.id"
    ))
    .fetch_all(conn)
    .await?;
    rows.into_iter().map(view).collect()
}

pub async fn exists(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM vacancies WHERE id = $1)")
        .bind(id)
        .fetch_one(conn)
        .await
}
