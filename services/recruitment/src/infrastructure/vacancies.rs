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
    client_id: Option<Uuid>,
    contact_id: Option<Uuid>,
    city: Option<String>,
    openings: i32,
    priority: String,
    recruiter: Option<String>,
    required_skills: Vec<String>,
    opened_on: NaiveDate,
    client_name: Option<String>,
    client_archived: Option<bool>,
    contact_name: Option<String>,
    contact_position: Option<String>,
    active_applications: i64,
    total_applications: i64,
}

/// Заказчик вакансии по проекции.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientInfo {
    pub id: Uuid,
    pub name: String,
    pub archived: bool,
}

/// Контактное лицо вакансии по проекции.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactInfo {
    pub id: Uuid,
    pub full_name: String,
    pub position: Option<String>,
}

/// Вакансия со счётчиками откликов и данными заказчика.
pub struct VacancyView {
    pub vacancy: Vacancy,
    pub client: Option<ClientInfo>,
    pub contact: Option<ContactInfo>,
    pub active_applications: i64,
    pub total_applications: i64,
}

const SELECT: &str = "
    SELECT v.id, v.tenant_id, v.title, v.client_id, v.contact_id, v.city, v.openings, v.priority,
           v.recruiter, v.required_skills, v.opened_on,
           (SELECT c.name FROM clients c WHERE c.client_id = v.client_id) AS client_name,
           (SELECT c.archived FROM clients c WHERE c.client_id = v.client_id) AS client_archived,
           (SELECT k.full_name FROM client_contacts k WHERE k.contact_id = v.contact_id) AS contact_name,
           (SELECT k.position FROM client_contacts k WHERE k.contact_id = v.contact_id) AS contact_position,
           count(a.id) FILTER (WHERE a.stage NOT IN ('hired', 'rejected')) AS active_applications,
           count(a.id) AS total_applications
    FROM vacancies v LEFT JOIN applications a ON a.vacancy_id = v.id";

fn view(row: VacancyRow) -> Result<VacancyView, StoreError> {
    let client = row
        .client_id
        .zip(row.client_name)
        .map(|(id, name)| ClientInfo {
            id,
            name,
            archived: row.client_archived.unwrap_or(false),
        });
    let contact = row
        .contact_id
        .zip(row.contact_name)
        .map(|(id, full_name)| ContactInfo {
            id,
            full_name,
            position: row.contact_position,
        });
    Ok(VacancyView {
        client,
        contact,
        vacancy: Vacancy::restore(
            row.id,
            TenantId(row.tenant_id),
            row.title,
            row.client_id,
            row.contact_id,
            row.city,
            row.openings,
            &row.priority,
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
        "INSERT INTO vacancies (id, tenant_id, title, client_id, contact_id, city, openings,
             priority, recruiter, required_skills, opened_on)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(v.id)
    .bind(v.tenant_id.0)
    .bind(&v.title)
    .bind(v.client_id)
    .bind(v.contact_id)
    .bind(&v.city)
    .bind(v.openings)
    .bind(v.priority.key())
    .bind(&v.recruiter)
    .bind(&v.required_skills)
    .bind(v.opened_on)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update(conn: &mut PgConnection, v: &Vacancy) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE vacancies SET title = $2, client_id = $3, contact_id = $4, city = $5, openings = $6,
             priority = $7, recruiter = $8, required_skills = $9, opened_on = $10,
             updated_at = now()
         WHERE id = $1",
    )
    .bind(v.id)
    .bind(&v.title)
    .bind(v.client_id)
    .bind(v.contact_id)
    .bind(&v.city)
    .bind(v.openings)
    .bind(v.priority.key())
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

/// Приоритетные выше, затем давно открытые; при `client_id` - только вакансии заказчика.
pub async fn list(
    conn: &mut PgConnection,
    client_id: Option<Uuid>,
) -> Result<Vec<VacancyView>, StoreError> {
    let rows: Vec<VacancyRow> = sqlx::query_as(&format!(
        "{SELECT} WHERE ($1::uuid IS NULL OR v.client_id = $1) GROUP BY v.id
         ORDER BY CASE v.priority WHEN 'high' THEN 0 WHEN 'normal' THEN 1 ELSE 2 END,
                  v.opened_on, v.title, v.id"
    ))
    .bind(client_id)
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
