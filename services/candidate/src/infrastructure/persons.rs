//! Хранение кандидатов. Все функции работают на соединении в контексте тенанта (RLS),
//! поэтому фильтр по `tenant_id` в запросах не нужен.
use chrono::{DateTime, NaiveDate, Utc};
use platform_core::tenant::TenantId;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::domain::{DomainError, Education, Employment, Person, PersonKeys, PersonRecord, Probe};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    /// В базе лежит значение, не проходящее проверки домена.
    #[error("stored person is inconsistent: {0}")]
    Corrupt(#[from] DomainError),
}

#[derive(FromRow)]
struct PersonRow {
    id: Uuid,
    tenant_id: Uuid,
    full_name: String,
    source: Option<String>,
    phone: Option<String>,
    email: Option<String>,
    telegram: Option<String>,
    whatsapp: Option<String>,
    expected_salary: Option<i64>,
    summary: Option<String>,
    skills: Vec<String>,
    declared_experience_years: i32,
    created_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct KeysRow {
    id: Uuid,
    full_name: String,
    normalized_name: String,
    normalized_email: Option<String>,
    normalized_phone: Option<String>,
}

#[derive(FromRow)]
struct EmploymentRow {
    company: String,
    position_title: String,
    start_date: NaiveDate,
    end_date: Option<NaiveDate>,
    description: Option<String>,
}

#[derive(FromRow)]
struct EducationRow {
    institution: String,
    specialty: Option<String>,
    year_from: Option<i32>,
    year_to: Option<i32>,
}

const PERSON_COLUMNS: &str = "id, tenant_id, full_name, source, phone, email, telegram, whatsapp, \
     expected_salary, summary, skills, declared_experience_years, created_at";

pub async fn insert(
    conn: &mut PgConnection,
    person: &Person,
    experience_years: i32,
) -> Result<(), sqlx::Error> {
    let keys = person.keys();
    sqlx::query(
        "INSERT INTO persons (id, tenant_id, full_name, source, phone, email, telegram, whatsapp,
             expected_salary, summary, skills, declared_experience_years, experience_years,
             normalized_name, normalized_email, normalized_phone, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
    )
    .bind(person.id)
    .bind(person.tenant_id.0)
    .bind(&person.full_name)
    .bind(&person.source)
    .bind(person.phone.as_ref().map(|p| p.as_str().to_string()))
    .bind(person.email.as_ref().map(|e| e.as_str().to_string()))
    .bind(person.telegram.as_ref().map(|t| t.as_str().to_string()))
    .bind(person.whatsapp.as_ref().map(|w| w.as_str().to_string()))
    .bind(person.expected_salary)
    .bind(&person.summary)
    .bind(&person.skills)
    .bind(person.declared_experience_years)
    .bind(experience_years)
    .bind(&keys.normalized_name)
    .bind(&keys.normalized_email)
    .bind(&keys.normalized_phone)
    .bind(person.created_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Претенденты на «того же человека»: совпадение хотя бы по одному из трёх ключей.
pub async fn find_candidates(
    conn: &mut PgConnection,
    probe: &Probe,
) -> Result<Vec<PersonKeys>, sqlx::Error> {
    let rows: Vec<KeysRow> = sqlx::query_as(
        "SELECT id, full_name, normalized_name, normalized_email, normalized_phone
         FROM persons
         WHERE ($1::text IS NOT NULL AND normalized_email = $1)
            OR ($2::text IS NOT NULL AND normalized_phone = $2)
            OR ($3 <> '' AND normalized_name = $3)",
    )
    .bind(&probe.normalized_email)
    .bind(&probe.normalized_phone)
    .bind(&probe.normalized_name)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| PersonKeys {
            id: r.id,
            full_name: r.full_name,
            normalized_name: r.normalized_name,
            normalized_email: r.normalized_email,
            normalized_phone: r.normalized_phone,
        })
        .collect())
}

/// Карточка со всей историей.
pub async fn get(conn: &mut PgConnection, id: Uuid) -> Result<Option<Person>, StoreError> {
    let row: Option<PersonRow> = sqlx::query_as(&format!(
        "SELECT {PERSON_COLUMNS} FROM persons WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };

    let employments: Vec<EmploymentRow> = sqlx::query_as(
        "SELECT company, position_title, start_date, end_date, description
         FROM employments WHERE person_id = $1 ORDER BY idx",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let educations: Vec<EducationRow> = sqlx::query_as(
        "SELECT institution, specialty, year_from, year_to
         FROM educations WHERE person_id = $1 ORDER BY idx",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;

    let employments = employments
        .into_iter()
        .map(|e| {
            Employment::new(
                &e.company,
                &e.position_title,
                e.start_date,
                e.end_date,
                e.description,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let educations = educations
        .into_iter()
        .map(|e| Education::new(&e.institution, e.specialty, e.year_from, e.year_to))
        .collect::<Result<Vec<_>, _>>()?;

    let record = PersonRecord {
        id: row.id,
        tenant_id: TenantId(row.tenant_id),
        full_name: row.full_name,
        source: row.source,
        phone: row.phone,
        email: row.email,
        telegram: row.telegram,
        whatsapp: row.whatsapp,
        expected_salary: row.expected_salary,
        summary: row.summary,
        skills: row.skills,
        declared_experience_years: row.declared_experience_years,
        created_at: row.created_at,
    };
    Ok(Some(Person::restore(record, employments, educations)?))
}
