//! Вызовы сервиса заказчиков через BFF (из браузера напрямую, при SSR — с cookie запроса).
use client_contracts::{
    ClientDetailDto, ClientDto, ContactDto, CreateClientRequest, CreateContactRequest, PositionDto,
    PositionRequest, UpdateClientRequest, UpdateContactRequest,
};
use leptos::prelude::*;
use ui_kit::api::Method;

use crate::{
    candidates::server::uuid_like,
    funnel::server::{Reply, classify, typed},
};

fn valid(id: &str) -> bool {
    id.parse::<uuid_like::Id>().is_ok()
}

/// Ответ без тела: успех — 200/204.
fn classify_empty(status: u16, body: &serde_json::Value) -> Reply<()> {
    match status {
        200 | 204 => Reply::Ok(()),
        other => classify(other, body, |_| Some(())),
    }
}

fn json_of<T: serde::Serialize>(value: &T) -> Result<serde_json::Value, ServerFnError> {
    serde_json::to_value(value).map_err(|e| ServerFnError::new(e.to_string()))
}

pub async fn list_clients(
    query: String,
    archived: bool,
) -> Result<Reply<Vec<ClientDto>>, ServerFnError> {
    use ui_kit::encode_query_value as enc;

    let mut path = String::from("/api/clients?");
    if !query.trim().is_empty() {
        path.push_str(&format!("q={}&", enc(query.trim())));
    }
    if archived {
        path.push_str("archived=true");
    }
    let reply = ui_kit::api::call(Method::Get, path.trim_end_matches(['?', '&']), None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn get_client(id: String) -> Result<Reply<ClientDetailDto>, ServerFnError> {
    if !valid(&id) {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::api::call(Method::Get, &format!("/api/clients/{id}"), None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn create_client(
    request: CreateClientRequest,
) -> Result<Reply<ClientDto>, ServerFnError> {
    let reply = ui_kit::api::call(Method::Post, "/api/clients", Some(json_of(&request)?)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn update_client(
    id: String,
    request: UpdateClientRequest,
) -> Result<Reply<ClientDto>, ServerFnError> {
    if !valid(&id) {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::api::call(
        Method::Patch,
        &format!("/api/clients/{id}"),
        Some(json_of(&request)?),
    )
    .await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn add_contact(
    client_id: String,
    request: CreateContactRequest,
) -> Result<Reply<ContactDto>, ServerFnError> {
    if !valid(&client_id) {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::api::call(
        Method::Post,
        &format!("/api/clients/{client_id}/contacts"),
        Some(json_of(&request)?),
    )
    .await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn update_contact(
    id: String,
    request: UpdateContactRequest,
) -> Result<Reply<ContactDto>, ServerFnError> {
    if !valid(&id) {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::api::call(
        Method::Patch,
        &format!("/api/contacts/{id}"),
        Some(json_of(&request)?),
    )
    .await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn delete_contact(id: String) -> Result<Reply<()>, ServerFnError> {
    if !valid(&id) {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::api::call(Method::Delete, &format!("/api/contacts/{id}"), None).await?;
    Ok(classify_empty(reply.status, &reply.body))
}

pub async fn list_positions() -> Result<Reply<Vec<PositionDto>>, ServerFnError> {
    let reply = ui_kit::api::call(Method::Get, "/api/positions", None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn create_position(name: String) -> Result<Reply<PositionDto>, ServerFnError> {
    let body = json_of(&PositionRequest { name })?;
    let reply = ui_kit::api::call(Method::Post, "/api/positions", Some(body)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn rename_position(
    id: String,
    name: String,
) -> Result<Reply<PositionDto>, ServerFnError> {
    if !valid(&id) {
        return Ok(Reply::NotFound);
    }
    let body = json_of(&PositionRequest { name })?;
    let reply =
        ui_kit::api::call(Method::Patch, &format!("/api/positions/{id}"), Some(body)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn delete_position(id: String) -> Result<Reply<()>, ServerFnError> {
    if !valid(&id) {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::api::call(Method::Delete, &format!("/api/positions/{id}"), None).await?;
    Ok(classify_empty(reply.status, &reply.body))
}

/// Текст ошибки у формы: конфликт заменяется понятной фразой, остальное — как есть.
pub fn failure_text<T>(reply: &Reply<T>, on_conflict: &str) -> Option<String> {
    match reply {
        Reply::Conflict(_) => Some(on_conflict.to_string()),
        other => other.failure(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn deletes_succeed_on_204_and_report_failures() {
        assert_eq!(classify_empty(204, &json!(null)), Reply::Ok(()));
        assert_eq!(classify_empty(404, &json!(null)), Reply::NotFound);
        assert!(matches!(
            classify_empty(409, &json!({"detail": "x"})),
            Reply::Conflict(_)
        ));
        assert_eq!(classify_empty(502, &json!(null)), Reply::Unavailable);
    }

    #[test]
    fn a_conflict_gets_the_callers_phrase() {
        let reply: Reply<()> = Reply::Conflict("position is assigned".into());
        assert_eq!(
            failure_text(&reply, "Должность используется").as_deref(),
            Some("Должность используется")
        );
        assert_eq!(failure_text(&Reply::Ok(()), "x"), None);
        assert!(failure_text(&Reply::<()>::Forbidden, "x").is_some());
    }

    #[test]
    fn malformed_ids_never_reach_the_network() {
        assert!(!valid("../etc"));
        assert!(!valid("12"));
        assert!(valid("00000000-0000-0000-0000-000000000001"));
    }
}
