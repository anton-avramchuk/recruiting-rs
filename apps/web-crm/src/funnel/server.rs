//! Server functions воронки: браузер -> web-crm -> bff-crm -> recruitment.
use leptos::prelude::*;
use leptos::server_fn::codec::Json;
use recruitment_contracts::{
    ApplicationDetailDto, ApplicationDto, ApplicationPage, BulkResult, CreateVacancyRequest,
    VacancyDto,
};
use serde::{Deserialize, Serialize};

/// Исход обращения к воронке в виде значения: интерфейс различает его без разбора HTTP.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Reply<T> {
    Ok(T),
    Invalid { field: String, message: String },
    Conflict(String),
    Forbidden,
    NotFound,
    Unavailable,
}

impl<T> Reply<T> {
    pub fn ok(self) -> Option<T> {
        match self {
            Reply::Ok(v) => Some(v),
            _ => None,
        }
    }

    /// Сообщение для пользователя, когда действие не удалось.
    pub fn failure(&self) -> Option<String> {
        Some(match self {
            Reply::Ok(_) => return None,
            Reply::Invalid { message, .. } => message.clone(),
            Reply::Conflict(m) => m.clone(),
            Reply::Forbidden => "Недостаточно прав для этого действия.".into(),
            Reply::NotFound => "Не найдено: возможно, запись уже удалена.".into(),
            Reply::Unavailable => "Сервис временно недоступен. Попробуйте позже.".into(),
        })
    }
}

/// Преобразует ответ BFF в `Reply`; `parse` достаёт данные из успешного тела.
pub fn classify<T>(
    status: u16,
    body: &serde_json::Value,
    parse: impl Fn(&serde_json::Value) -> Option<T>,
) -> Reply<T> {
    let text = |key: &str| body[key].as_str().unwrap_or_default().to_string();
    match status {
        200 | 201 => parse(body).map_or(Reply::Unavailable, Reply::Ok),
        400 => Reply::Invalid {
            field: text("field"),
            message: match text("detail") {
                d if d.is_empty() => "Проверьте введённые данные".into(),
                d => d,
            },
        },
        401 | 403 => Reply::Forbidden,
        404 => Reply::NotFound,
        409 => Reply::Conflict(match text("detail") {
            d if d.is_empty() => "Действие сейчас недоступно".into(),
            d => d,
        }),
        _ => Reply::Unavailable,
    }
}

#[cfg(feature = "ssr")]
fn typed<T: serde::de::DeserializeOwned>(body: &serde_json::Value) -> Option<T> {
    serde_json::from_value(body.clone()).ok()
}

/// Параметры списка откликов; пустые строки не ограничивают выборку.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppQuery {
    pub vacancy_id: String,
    pub stage: String,
    pub source: String,
    pub q: String,
    pub sort: String,
    pub desc: bool,
    pub page: i64,
    pub page_size: i64,
}

#[server(input = Json)]
pub async fn list_vacancies() -> Result<Reply<Vec<VacancyDto>>, ServerFnError> {
    use axum::http::Method;
    let reply = ui_kit::server::call(Method::GET, "/api/vacancies", None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

#[server(input = Json)]
pub async fn create_vacancy(
    request: CreateVacancyRequest,
) -> Result<Reply<VacancyDto>, ServerFnError> {
    use axum::http::Method;
    let body = serde_json::to_value(&request).map_err(|e| ServerFnError::new(e.to_string()))?;
    let reply = ui_kit::server::call(Method::POST, "/api/vacancies", Some(body)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

#[server(input = Json)]
pub async fn list_applications(query: AppQuery) -> Result<Reply<ApplicationPage>, ServerFnError> {
    use axum::http::Method;
    use ui_kit::encode_query_value as enc;

    let page_size = if query.page_size > 0 {
        query.page_size
    } else {
        50
    };
    let mut path = format!(
        "/api/applications?page={}&page_size={page_size}",
        query.page.max(1)
    );
    for (name, value) in [
        ("vacancy_id", query.vacancy_id.trim()),
        ("stage", query.stage.trim()),
        ("source", query.source.trim()),
        ("q", query.q.trim()),
        ("sort", query.sort.trim()),
    ] {
        if !value.is_empty() {
            path.push_str(&format!("&{name}={}", enc(value)));
        }
    }
    if !query.sort.trim().is_empty() {
        path.push_str(if query.desc { "&dir=desc" } else { "&dir=asc" });
    }
    let reply = ui_kit::server::call(Method::GET, &path, None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

#[server(input = Json)]
pub async fn get_application(id: String) -> Result<Reply<ApplicationDetailDto>, ServerFnError> {
    use axum::http::Method;
    if id
        .parse::<crate::candidates::server::uuid_like::Id>()
        .is_err()
    {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::server::call(Method::GET, &format!("/api/applications/{id}"), None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

#[server(input = Json)]
pub async fn create_application(
    person_id: String,
    vacancy_id: String,
    stage: String,
) -> Result<Reply<ApplicationDto>, ServerFnError> {
    use axum::http::Method;
    use serde_json::json;
    let mut body = json!({"person_id": person_id, "vacancy_id": vacancy_id});
    if !stage.trim().is_empty() {
        body["stage"] = json!(stage.trim());
    }
    let reply = ui_kit::server::call(Method::POST, "/api/applications", Some(body)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

/// Действие над откликом: `stage`, `next`, `reject`, `interviews`, `handoff`.
#[server(input = Json)]
pub async fn application_action(
    id: String,
    action: String,
    body: serde_json::Value,
) -> Result<Reply<ApplicationDto>, ServerFnError> {
    use axum::http::Method;
    if id
        .parse::<crate::candidates::server::uuid_like::Id>()
        .is_err()
    {
        return Ok(Reply::NotFound);
    }
    if !["stage", "next", "reject", "interviews", "handoff"].contains(&action.as_str()) {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::server::call(
        Method::POST,
        &format!("/api/applications/{id}/{action}"),
        Some(body),
    )
    .await?;
    Ok(classify(reply.status, &reply.body, typed))
}

/// Групповое действие: `next`, `reject` или `letter`.
#[server(input = Json)]
pub async fn bulk_applications(
    action: String,
    ids: Vec<String>,
    reason: String,
) -> Result<Reply<BulkResult>, ServerFnError> {
    use axum::http::Method;
    use serde_json::json;
    let mut body = json!({"action": action, "ids": ids});
    if !reason.trim().is_empty() {
        body["reason"] = json!(reason.trim());
    }
    let reply = ui_kit::server::call(Method::POST, "/api/applications/bulk", Some(body)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

/// Заметка рекрутера о человеке (хранится в карточке кандидата).
#[server(input = Json)]
pub async fn save_note(person_id: String, note: String) -> Result<Reply<()>, ServerFnError> {
    use axum::http::Method;
    use serde_json::json;
    if person_id
        .parse::<crate::candidates::server::uuid_like::Id>()
        .is_err()
    {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::server::call(
        Method::PATCH,
        &format!("/api/persons/{person_id}"),
        Some(json!({"note": note})),
    )
    .await?;
    Ok(classify(reply.status, &reply.body, |_| Some(())))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn statuses_become_values() {
        let ok: Reply<u8> = classify(200, &json!(7), |v| v.as_u64().map(|n| n as u8));
        assert_eq!(ok, Reply::Ok(7));
        assert_eq!(
            classify::<u8>(200, &json!("x"), |_| None),
            Reply::Unavailable
        );
        assert_eq!(
            classify::<u8>(400, &json!({"field": "at", "detail": "прошло"}), |_| None),
            Reply::Invalid {
                field: "at".into(),
                message: "прошло".into()
            }
        );
        assert_eq!(
            classify::<u8>(403, &json!(null), |_| None),
            Reply::Forbidden
        );
        assert_eq!(classify::<u8>(404, &json!(null), |_| None), Reply::NotFound);
        assert!(matches!(
            classify::<u8>(409, &json!({}), |_| None),
            Reply::Conflict(_)
        ));
        assert_eq!(
            classify::<u8>(502, &json!(null), |_| None),
            Reply::Unavailable
        );
    }

    #[test]
    fn failure_message_exists_only_for_failures() {
        assert!(Reply::Ok(1).failure().is_none());
        assert!(Reply::<u8>::Forbidden.failure().is_some());
        assert_eq!(
            Reply::<u8>::Conflict("нельзя".into()).failure().as_deref(),
            Some("нельзя")
        );
    }
}
