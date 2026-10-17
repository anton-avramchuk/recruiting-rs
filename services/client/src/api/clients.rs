use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use client_contracts::{
    ClientDetailDto, ClientDto, ContactDto, CreateClientRequest, CreateContactRequest,
    UpdateClientRequest,
};
use grid_contracts::{GridPage, GridQuery};
use platform_auth::{AuthUser, Permission};
use platform_core::error::{ApiError, ProblemDetails};
use serde::Deserialize;
use uuid::Uuid;

use super::{
    AppState,
    error::tenant_of,
    mapping::{client_to_dto, contact_to_dto},
};
use crate::domain::{ClientPatch, NewClient, NewContact};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/clients", post(create).get(list))
        .route("/clients/grid", get(grid))
        .route("/clients/{id}", get(get_one).patch(update))
        .route("/clients/{id}/contacts", post(add_contact))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct ListParams {
    /// Поиск по названию (вхождение без учёта регистра).
    q: Option<String>,
    /// Включить архивных заказчиков.
    #[serde(default)]
    archived: bool,
}

#[utoipa::path(post, path = "/clients", security(("bearer" = [])), request_body = CreateClientRequest, responses(
    (status = 201, body = ClientDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateClientRequest>,
) -> Result<(StatusCode, Json<ClientDto>), ApiError> {
    user.require(Permission::WriteData)?;
    let row = state
        .clients
        .create(
            tenant_of(&user)?,
            NewClient {
                name: req.name,
                note: req.note,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(client_to_dto(&row))))
}

#[utoipa::path(get, path = "/clients", security(("bearer" = [])), params(ListParams), responses(
    (status = 200, body = Vec<ClientDto>),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<ClientDto>>, ApiError> {
    user.require(Permission::ReadData)?;
    let rows = state
        .clients
        .list(tenant_of(&user)?, params.q.as_deref(), params.archived)
        .await?;
    Ok(Json(rows.iter().map(client_to_dto).collect()))
}

/// Параметры списка: `page`, `page_size`, `sort` (`поле:asc,поле:desc`), `filter` (JSON).
#[utoipa::path(get, path = "/clients/grid", security(("bearer" = [])), params(
    ("page" = Option<u32>, Query, description = "Страница, с 1"),
    ("page_size" = Option<u32>, Query, description = "Размер страницы, до 100"),
    ("sort" = Option<String>, Query, description = "Сортировка: поле:asc,поле:desc"),
    ("filter" = Option<String>, Query, description = "Фильтр грида (JSON)"),
), responses(
    (status = 200, body = GridPage<ClientDto>),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn grid(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<Vec<(String, String)>>,
) -> Result<Json<GridPage<ClientDto>>, ApiError> {
    user.require(Permission::ReadData)?;
    let query = GridQuery::from_pairs(params.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .map_err(platform_grid::GridError::from)?;
    let page = state.clients.page(tenant_of(&user)?, query).await?;
    Ok(Json(GridPage {
        items: page.items.iter().map(client_to_dto).collect(),
        total: page.total,
        page: page.page,
        page_size: page.page_size,
    }))
}

#[utoipa::path(get, path = "/clients/{id}", security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Client id")), responses(
    (status = 200, body = ClientDetailDto),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ClientDetailDto>, ApiError> {
    user.require(Permission::ReadData)?;
    let detail = state.clients.get(tenant_of(&user)?, id).await?;
    Ok(Json(ClientDetailDto {
        client: client_to_dto(&detail.client),
        contacts: detail.contacts.iter().map(contact_to_dto).collect(),
    }))
}

#[utoipa::path(patch, path = "/clients/{id}", security(("bearer" = [])), request_body = UpdateClientRequest,
    params(("id" = Uuid, Path, description = "Client id")), responses(
    (status = 200, body = ClientDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateClientRequest>,
) -> Result<Json<ClientDto>, ApiError> {
    user.require(Permission::WriteData)?;
    let row = state
        .clients
        .update(
            tenant_of(&user)?,
            id,
            ClientPatch {
                name: req.name,
                note: req.note,
                archived: req.archived,
            },
        )
        .await?;
    Ok(Json(client_to_dto(&row)))
}

#[utoipa::path(post, path = "/clients/{id}/contacts", security(("bearer" = [])), request_body = CreateContactRequest,
    params(("id" = Uuid, Path, description = "Client id")), responses(
    (status = 201, body = ContactDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn add_contact(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateContactRequest>,
) -> Result<(StatusCode, Json<ContactDto>), ApiError> {
    user.require(Permission::WriteData)?;
    let row = state
        .clients
        .add_contact(
            tenant_of(&user)?,
            id,
            NewContact {
                full_name: req.full_name,
                position_id: req.position_id,
                email: req.email,
                phone: req.phone,
                note: req.note,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(contact_to_dto(&row))))
}
