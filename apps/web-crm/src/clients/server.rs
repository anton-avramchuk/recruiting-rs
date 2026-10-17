//! Вызовы сервиса заказчиков через BFF (из браузера напрямую, при SSR — с cookie запроса).
use client_contracts::{
    ClientDetailDto, ClientDto, ContactDto, CreateClientRequest, CreateContactRequest, PositionDto,
    PositionRequest, UpdateClientRequest, UpdateContactRequest,
};
use leptos::prelude::*;
use ui_kit::{Reply, api::Method, classify, classify_empty, typed};

use crate::candidates::server::uuid_like;

fn valid(id: &str) -> bool {
    id.parse::<uuid_like::Id>().is_ok()
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

/// Страница заказчиков по запросу грида (по умолчанию только активные).
pub async fn list_clients_grid(
    query: grid_contracts::GridQuery,
) -> Result<Reply<grid_contracts::GridPage<ClientDto>>, ServerFnError> {
    let path = ui_kit::grid::grid_path("/api/clients/grid", &query);
    let reply = ui_kit::api::call(Method::Get, &path, None).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_ids_never_reach_the_network() {
        assert!(!valid("../etc"));
        assert!(!valid("12"));
        assert!(valid("00000000-0000-0000-0000-000000000001"));
    }
}
