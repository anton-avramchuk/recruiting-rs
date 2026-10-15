//! Server functions раздела «Кандидаты»: браузер -> web-crm -> bff-crm -> candidate.
// поля HTML-формы приходят отдельными аргументами server function
#![allow(clippy::too_many_arguments)]

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use ui_kit::api::Method;

use candidate_contracts::{EducationDto, EmploymentDto};

use super::outcome::{CreateOutcome, DeleteOutcome, ListOutcome, LoadOutcome, SaveOutcome};

/// Поиск и фильтры списка. Пустые строки не ограничивают выборку.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonQuery {
    pub q: String,
    pub source: String,
    pub skill: String,
    pub min_experience: String,
    pub page: i64,
}

pub async fn search_persons(query: PersonQuery) -> Result<ListOutcome, ServerFnError> {
    use ui_kit::encode_query_value as enc;

    let mut path = format!("/api/persons?page={}", query.page.max(1));
    for (name, value) in [
        ("q", query.q.trim()),
        ("source", query.source.trim()),
        ("skill", query.skill.trim()),
    ] {
        if !value.is_empty() {
            path.push_str(&format!("&{name}={}", enc(value)));
        }
    }
    if let Some(min) = super::format::parse_optional_number(&query.min_experience) {
        path.push_str(&format!("&min_experience={min}"));
    }
    let reply = ui_kit::api::call(Method::Get, &path, None).await?;
    Ok(super::outcome::classify_list(reply.status, &reply.body))
}

pub async fn get_person(id: String) -> Result<LoadOutcome, ServerFnError> {
    if id.parse::<uuid_like::Id>().is_err() {
        return Ok(LoadOutcome::NotFound);
    }
    let reply = ui_kit::api::call(Method::Get, &format!("/api/persons/{id}"), None).await?;
    Ok(super::outcome::classify_load(reply.status, &reply.body))
}

pub async fn create_person(
    full_name: String,
    phone: String,
    email: String,
    telegram: String,
    whatsapp: String,
    source: String,
    expected_salary: String,
    skills: String,
    summary: String,
    city: String,
    declared_experience_years: String,
    confirm_duplicate: String,
) -> Result<CreateOutcome, ServerFnError> {
    use serde_json::{Map, Value, json};

    use super::format::{parse_optional_number, split_skills};

    let mut body = Map::new();
    body.insert("full_name".into(), json!(full_name.trim()));
    for (key, value) in [
        ("phone", &phone),
        ("email", &email),
        ("telegram", &telegram),
        ("whatsapp", &whatsapp),
        ("source", &source),
        ("summary", &summary),
        ("city", &city),
    ] {
        if !value.trim().is_empty() {
            body.insert(key.into(), json!(value.trim()));
        }
    }
    for (key, raw) in [
        ("expected_salary", &expected_salary),
        ("declared_experience_years", &declared_experience_years),
    ] {
        if raw.trim().is_empty() {
            continue;
        }
        match parse_optional_number(raw) {
            Some(n) => {
                body.insert(key.into(), json!(n));
            }
            None => {
                return Ok(CreateOutcome::Invalid {
                    field: key.into(),
                    message: "Введите число".into(),
                });
            }
        }
    }
    body.insert("skills".into(), json!(split_skills(&skills)));
    body.insert(
        "confirm_duplicate".into(),
        json!(confirm_duplicate == "true"),
    );

    let reply = ui_kit::api::call(Method::Post, "/api/persons", Some(Value::Object(body))).await?;
    Ok(super::outcome::classify_create(reply.status, &reply.body))
}

pub async fn update_person(
    id: String,
    full_name: String,
    phone: String,
    email: String,
    telegram: String,
    whatsapp: String,
    source: String,
    expected_salary: String,
    skills: String,
    summary: String,
    city: String,
) -> Result<SaveOutcome, ServerFnError> {
    use serde_json::{Map, Value, json};

    use super::format::{parse_optional_number, split_skills};

    if id.parse::<uuid_like::Id>().is_err() {
        return Ok(SaveOutcome::NotFound);
    }
    // пустая строка очищает необязательное поле: на сервер уходит null
    let nullable = |s: &str| {
        if s.trim().is_empty() {
            Value::Null
        } else {
            json!(s.trim())
        }
    };
    let salary = if expected_salary.trim().is_empty() {
        Value::Null
    } else {
        match parse_optional_number(&expected_salary) {
            Some(n) => json!(n),
            None => {
                return Ok(SaveOutcome::Invalid {
                    field: "expected_salary".into(),
                    message: "Введите число".into(),
                });
            }
        }
    };

    let mut body = Map::new();
    body.insert("full_name".into(), json!(full_name.trim()));
    body.insert("phone".into(), nullable(&phone));
    body.insert("email".into(), nullable(&email));
    body.insert("telegram".into(), nullable(&telegram));
    body.insert("whatsapp".into(), nullable(&whatsapp));
    body.insert("source".into(), nullable(&source));
    body.insert("summary".into(), nullable(&summary));
    body.insert("city".into(), nullable(&city));
    body.insert("expected_salary".into(), salary);
    body.insert("skills".into(), json!(split_skills(&skills)));

    let reply = ui_kit::api::call(
        Method::Patch,
        &format!("/api/persons/{id}"),
        Some(Value::Object(body)),
    )
    .await?;
    Ok(super::outcome::classify_save(reply.status, &reply.body))
}

pub async fn save_employments(
    id: String,
    items: Vec<EmploymentDto>,
) -> Result<SaveOutcome, ServerFnError> {
    if id.parse::<uuid_like::Id>().is_err() {
        return Ok(SaveOutcome::NotFound);
    }
    let body = serde_json::to_value(&items).map_err(|e| ServerFnError::new(e.to_string()))?;
    let reply = ui_kit::api::call(
        Method::Put,
        &format!("/api/persons/{id}/employments"),
        Some(body),
    )
    .await?;
    Ok(super::outcome::classify_save(reply.status, &reply.body))
}

pub async fn save_educations(
    id: String,
    items: Vec<EducationDto>,
) -> Result<SaveOutcome, ServerFnError> {
    if id.parse::<uuid_like::Id>().is_err() {
        return Ok(SaveOutcome::NotFound);
    }
    let body = serde_json::to_value(&items).map_err(|e| ServerFnError::new(e.to_string()))?;
    let reply = ui_kit::api::call(
        Method::Put,
        &format!("/api/persons/{id}/educations"),
        Some(body),
    )
    .await?;
    Ok(super::outcome::classify_save(reply.status, &reply.body))
}

pub async fn delete_person(id: String) -> Result<DeleteOutcome, ServerFnError> {
    if id.parse::<uuid_like::Id>().is_err() {
        return Ok(DeleteOutcome::NotFound);
    }
    let reply = ui_kit::api::call(Method::Delete, &format!("/api/persons/{id}"), None).await?;
    Ok(super::outcome::classify_delete(reply.status))
}

/// Проверка формата идентификатора без лишней зависимости: в путь запроса попадает только UUID.
pub(crate) mod uuid_like {
    use std::str::FromStr;

    pub struct Id;

    impl FromStr for Id {
        type Err = ();

        fn from_str(s: &str) -> Result<Self, ()> {
            let ok = s.len() == 36
                && s.bytes().enumerate().all(|(i, b)| match i {
                    8 | 13 | 18 | 23 => b == b'-',
                    _ => b.is_ascii_hexdigit(),
                });
            if ok { Ok(Id) } else { Err(()) }
        }
    }
}

// ---------- аргументы действий форм ----------

/// Значения поля формы или пустая строка.
fn field(fields: &ui_kit::FormFields, name: &str) -> String {
    fields.get(name).cloned().unwrap_or_default()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreatePerson {
    pub full_name: String,
    pub phone: String,
    pub email: String,
    pub telegram: String,
    pub whatsapp: String,
    pub source: String,
    pub expected_salary: String,
    pub skills: String,
    pub summary: String,
    pub city: String,
    pub declared_experience_years: String,
    pub confirm_duplicate: String,
}

impl CreatePerson {
    pub fn from_fields(f: &ui_kit::FormFields) -> Self {
        Self {
            full_name: field(f, "full_name"),
            phone: field(f, "phone"),
            email: field(f, "email"),
            telegram: field(f, "telegram"),
            whatsapp: field(f, "whatsapp"),
            source: field(f, "source"),
            expected_salary: field(f, "expected_salary"),
            skills: field(f, "skills"),
            summary: field(f, "summary"),
            city: field(f, "city"),
            declared_experience_years: field(f, "declared_experience_years"),
            confirm_duplicate: field(f, "confirm_duplicate"),
        }
    }

    pub async fn run(self) -> Result<CreateOutcome, ServerFnError> {
        create_person(
            self.full_name,
            self.phone,
            self.email,
            self.telegram,
            self.whatsapp,
            self.source,
            self.expected_salary,
            self.skills,
            self.summary,
            self.city,
            self.declared_experience_years,
            self.confirm_duplicate,
        )
        .await
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdatePerson {
    pub id: String,
    pub full_name: String,
    pub phone: String,
    pub email: String,
    pub telegram: String,
    pub whatsapp: String,
    pub source: String,
    pub expected_salary: String,
    pub skills: String,
    pub summary: String,
    pub city: String,
}

impl UpdatePerson {
    pub fn from_fields(f: &ui_kit::FormFields) -> Self {
        Self {
            id: field(f, "id"),
            full_name: field(f, "full_name"),
            phone: field(f, "phone"),
            email: field(f, "email"),
            telegram: field(f, "telegram"),
            whatsapp: field(f, "whatsapp"),
            source: field(f, "source"),
            expected_salary: field(f, "expected_salary"),
            skills: field(f, "skills"),
            summary: field(f, "summary"),
            city: field(f, "city"),
        }
    }

    pub async fn run(self) -> Result<SaveOutcome, ServerFnError> {
        update_person(
            self.id,
            self.full_name,
            self.phone,
            self.email,
            self.telegram,
            self.whatsapp,
            self.source,
            self.expected_salary,
            self.skills,
            self.summary,
            self.city,
        )
        .await
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveEmployments {
    pub id: String,
    pub items: Vec<EmploymentDto>,
}

impl SaveEmployments {
    pub async fn run(self) -> Result<SaveOutcome, ServerFnError> {
        save_employments(self.id, self.items).await
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveEducations {
    pub id: String,
    pub items: Vec<EducationDto>,
}

impl SaveEducations {
    pub async fn run(self) -> Result<SaveOutcome, ServerFnError> {
        save_educations(self.id, self.items).await
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeletePerson {
    pub id: String,
}

impl DeletePerson {
    pub async fn run(self) -> Result<DeleteOutcome, ServerFnError> {
        delete_person(self.id).await
    }
}

/// Действие над кандидатом: аргументы -> исход (замена `ServerAction`).
pub fn person_action<I, O, Fut>(run: fn(I) -> Fut) -> Action<I, Result<O, ServerFnError>>
where
    I: Clone + Send + Sync + 'static,
    O: Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<O, ServerFnError>> + Send + 'static,
{
    Action::new(move |input: &I| run(input.clone()))
}
