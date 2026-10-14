//! Server functions раздела «Кандидаты»: браузер -> web-crm -> bff-crm -> candidate.
use leptos::prelude::*;
use leptos::server_fn::codec::Json;
use serde::{Deserialize, Serialize};

use super::outcome::{CreateOutcome, ListOutcome, LoadOutcome};

/// Поиск и фильтры списка. Пустые строки не ограничивают выборку.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonQuery {
    pub q: String,
    pub source: String,
    pub skill: String,
    pub min_experience: String,
    pub page: i64,
}

#[server(input = Json)]
pub async fn search_persons(query: PersonQuery) -> Result<ListOutcome, ServerFnError> {
    use axum::http::Method;
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
    let reply = ui_kit::server::call(Method::GET, &path, None).await?;
    Ok(super::outcome::classify_list(reply.status, &reply.body))
}

#[server]
pub async fn get_person(id: String) -> Result<LoadOutcome, ServerFnError> {
    use axum::http::Method;
    if id.parse::<uuid_like::Id>().is_err() {
        return Ok(LoadOutcome::NotFound);
    }
    let reply = ui_kit::server::call(Method::GET, &format!("/api/persons/{id}"), None).await?;
    Ok(super::outcome::classify_load(reply.status, &reply.body))
}

#[server]
#[allow(clippy::too_many_arguments)]
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
    declared_experience_years: String,
    confirm_duplicate: String,
) -> Result<CreateOutcome, ServerFnError> {
    use axum::http::Method;
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

    let reply =
        ui_kit::server::call(Method::POST, "/api/persons", Some(Value::Object(body))).await?;
    Ok(super::outcome::classify_create(reply.status, &reply.body))
}

/// Проверка формата идентификатора без лишней зависимости: в путь запроса попадает только UUID.
#[cfg(feature = "ssr")]
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
