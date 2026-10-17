//! Server functions воронки: браузер -> web-crm -> bff-crm -> recruitment.
use leptos::prelude::*;
use recruitment_contracts::{
    AnalyticsDto, ApplicationDetailDto, ApplicationDto, ApplicationPage, BatchApplicationsRequest,
    BatchApplicationsResponse, BulkResult, CreateVacancyRequest, InterviewItemDto, OverviewDto,
    VacancyDto,
};
use serde::{Deserialize, Serialize};
use ui_kit::{Reply, api::Method, classify, typed};

/// Параметры списка откликов; пустые строки не ограничивают выборку.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppQuery {
    pub vacancy_id: String,
    /// Заказчик вакансии; пусто - все.
    pub client_id: String,
    pub stage: String,
    pub source: String,
    pub q: String,
    pub sort: String,
    pub desc: bool,
    pub page: i64,
    pub page_size: i64,
    /// Только просроченные по SLA.
    pub overdue: bool,
}

pub async fn list_vacancies() -> Result<Reply<Vec<VacancyDto>>, ServerFnError> {
    list_vacancies_of(String::new()).await
}

/// Вакансии организации; непустой `client_id` оставляет вакансии одного заказчика.
pub async fn list_vacancies_of(client_id: String) -> Result<Reply<Vec<VacancyDto>>, ServerFnError> {
    let client_id = client_id.trim();
    let mut path = String::from("/api/vacancies");
    if !client_id.is_empty() {
        if client_id
            .parse::<crate::candidates::server::uuid_like::Id>()
            .is_err()
        {
            return Ok(Reply::NotFound);
        }
        path.push_str(&format!("?client_id={client_id}"));
    }
    let reply = ui_kit::api::call(Method::Get, &path, None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn create_vacancy(
    request: CreateVacancyRequest,
) -> Result<Reply<VacancyDto>, ServerFnError> {
    let body = serde_json::to_value(&request).map_err(|e| ServerFnError::new(e.to_string()))?;
    let reply = ui_kit::api::call(Method::Post, "/api/vacancies", Some(body)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn list_applications(query: AppQuery) -> Result<Reply<ApplicationPage>, ServerFnError> {
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
        ("client_id", query.client_id.trim()),
        ("stage", query.stage.trim()),
        ("source", query.source.trim()),
        ("q", query.q.trim()),
        ("sort", query.sort.trim()),
    ] {
        if !value.is_empty() {
            path.push_str(&format!("&{name}={}", enc(value)));
        }
    }
    if query.overdue {
        path.push_str("&overdue=true");
    }
    if !query.sort.trim().is_empty() {
        path.push_str(if query.desc { "&dir=desc" } else { "&dir=asc" });
    }
    let reply = ui_kit::api::call(Method::Get, &path, None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

/// Метрики найма за период в сутках (7…365).
/// Метрики за период; непустой `client_id` сужает их до вакансий одного заказчика.
pub async fn get_analytics(
    days: i64,
    client_id: String,
) -> Result<Reply<AnalyticsDto>, ServerFnError> {
    let mut path = format!("/api/analytics?days={days}");
    let client_id = client_id.trim();
    if !client_id.is_empty() {
        if client_id
            .parse::<crate::candidates::server::uuid_like::Id>()
            .is_err()
        {
            return Ok(Reply::NotFound);
        }
        path.push_str(&format!("&client_id={client_id}"));
    }
    let reply = ui_kit::api::call(Method::Get, &path, None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn get_overview() -> Result<Reply<OverviewDto>, ServerFnError> {
    let reply = ui_kit::api::call(Method::Get, "/api/overview", None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

/// Интервью за период `[from, to)`; границы — RFC 3339 в часовом поясе пользователя.
pub async fn list_interviews(
    from: String,
    to: String,
) -> Result<Reply<Vec<InterviewItemDto>>, ServerFnError> {
    use ui_kit::encode_query_value as enc;
    let path = format!("/api/interviews?from={}&to={}", enc(&from), enc(&to));
    let reply = ui_kit::api::call(Method::Get, &path, None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn get_application(id: String) -> Result<Reply<ApplicationDetailDto>, ServerFnError> {
    if id
        .parse::<crate::candidates::server::uuid_like::Id>()
        .is_err()
    {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::api::call(Method::Get, &format!("/api/applications/{id}"), None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

/// Всё, что нужно карточке режима разбора, одним обращением браузера.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TriageData {
    pub detail: Reply<ApplicationDetailDto>,
    pub person: Reply<candidate_contracts::PersonDto>,
}

pub async fn triage_card(id: String, person_id: String) -> Result<TriageData, ServerFnError> {
    use crate::candidates::server::uuid_like::Id;

    if id.parse::<Id>().is_err() || person_id.parse::<Id>().is_err() {
        return Ok(TriageData {
            detail: Reply::NotFound,
            person: Reply::NotFound,
        });
    }
    let detail = ui_kit::api::call(Method::Get, &format!("/api/applications/{id}"), None).await?;
    let person = ui_kit::api::call(Method::Get, &format!("/api/persons/{person_id}"), None).await?;
    Ok(TriageData {
        detail: classify(detail.status, &detail.body, typed),
        person: classify(person.status, &person.body, typed),
    })
}

pub async fn create_application(
    person_id: String,
    vacancy_id: String,
    stage: String,
) -> Result<Reply<ApplicationDto>, ServerFnError> {
    use serde_json::json;
    let mut body = json!({"person_id": person_id, "vacancy_id": vacancy_id});
    if !stage.trim().is_empty() {
        body["stage"] = json!(stage.trim());
    }
    let reply = ui_kit::api::call(Method::Post, "/api/applications", Some(body)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

/// Действие над откликом: `stage`, `next`, `reject`, `interviews`, `handoff`.
pub async fn application_action(
    id: String,
    action: String,
    body: serde_json::Value,
) -> Result<Reply<ApplicationDto>, ServerFnError> {
    if id
        .parse::<crate::candidates::server::uuid_like::Id>()
        .is_err()
    {
        return Ok(Reply::NotFound);
    }
    if !["stage", "next", "reject", "interviews", "handoff"].contains(&action.as_str()) {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::api::call(
        Method::Post,
        &format!("/api/applications/{id}/{action}"),
        Some(body),
    )
    .await?;
    Ok(classify(reply.status, &reply.body, typed))
}

/// Отклики на вакансию для набора людей (импорт).
pub async fn create_applications_batch(
    request: BatchApplicationsRequest,
) -> Result<Reply<BatchApplicationsResponse>, ServerFnError> {
    let body = serde_json::to_value(&request).map_err(|e| ServerFnError::new(e.to_string()))?;
    let reply = ui_kit::api::call(Method::Post, "/api/applications/batch", Some(body)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

/// Групповое действие: `next`, `reject` или `letter`.
pub async fn bulk_applications(
    action: String,
    ids: Vec<String>,
    reason: String,
) -> Result<Reply<BulkResult>, ServerFnError> {
    use serde_json::json;
    let mut body = json!({"action": action, "ids": ids});
    if !reason.trim().is_empty() {
        body["reason"] = json!(reason.trim());
    }
    let reply = ui_kit::api::call(Method::Post, "/api/applications/bulk", Some(body)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

/// Заметка рекрутера о человеке (хранится в карточке кандидата).
pub async fn save_note(person_id: String, note: String) -> Result<Reply<()>, ServerFnError> {
    use serde_json::json;
    if person_id
        .parse::<crate::candidates::server::uuid_like::Id>()
        .is_err()
    {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::api::call(
        Method::Patch,
        &format!("/api/persons/{person_id}"),
        Some(json!({"note": note})),
    )
    .await?;
    Ok(classify(reply.status, &reply.body, |_| Some(())))
}
