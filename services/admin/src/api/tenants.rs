use admin_contracts::{CreateTenantRequest, TenantDto, TenantPage};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use platform_auth::{AuthUser, Permission};
use platform_core::{
    error::{ApiError, ProblemDetails},
    tenant::TenantId,
};
use platform_grid::contracts::GridQuery;
use serde::Deserialize;
use uuid::Uuid;

use super::AppState;
use crate::{
    application::{
        provisioning::OwnerSpec,
        tenants::{DEFAULT_PAGE_SIZE, ListQuery, NewTenant, TenantError},
    },
    domain::{DomainError, Tenant, TenantStatus},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/tenants", post(create).get(list))
        .route("/tenants/{id}", get(get_one))
        .route("/tenants/{id}/suspend", post(suspend))
        .route("/tenants/{id}/activate", post(activate))
}

pub(super) fn to_dto(t: Tenant) -> TenantDto {
    TenantDto {
        id: t.id.0,
        slug: t.slug.as_str().to_string(),
        name: t.name,
        status: t.status.as_str().to_string(),
    }
}

#[utoipa::path(post, path = "/tenants", security(("bearer" = [])), request_body = CreateTenantRequest, responses(
    (status = 201, body = TenantDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 502, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateTenantRequest>,
) -> Result<(StatusCode, Json<TenantDto>), ApiError> {
    user.require(Permission::ManageTenants)?;
    let tenant = state
        .tenants
        .create(NewTenant {
            slug: req.slug,
            name: req.name,
            owner: OwnerSpec {
                email: req.owner.email,
                password: req.owner.password,
                display_name: req.owner.display_name,
            },
        })
        .await?;
    Ok((StatusCode::CREATED, Json(to_dto(tenant))))
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct ListParams {
    status: Option<String>,
    q: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
    /// Сортировка грида: `поле:asc,поле:desc`. Вместе с `filter` включает режим грида.
    sort: Option<String>,
    /// Фильтр грида (JSON). В режиме грида `status` и `q` не применяются.
    filter: Option<String>,
}

#[utoipa::path(get, path = "/tenants", security(("bearer" = [])), params(ListParams), responses(
    (status = 200, body = TenantPage),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<ListParams>,
) -> Result<Json<TenantPage>, ApiError> {
    user.require(Permission::ManageTenants)?;
    if params.sort.is_some() || params.filter.is_some() {
        return list_grid(&state, &params).await;
    }
    let status = params
        .status
        .map(|s| TenantStatus::parse(&s).ok_or_else(|| ApiError::bad_request("unknown status")))
        .transpose()?;
    let page = state
        .tenants
        .list(ListQuery {
            status,
            q: params.q,
            page: params.page.unwrap_or(1),
            page_size: params.page_size.unwrap_or(DEFAULT_PAGE_SIZE),
        })
        .await?;
    Ok(Json(TenantPage {
        items: page.items.into_iter().map(to_dto).collect(),
        total: page.total,
        page: page.page,
        page_size: page.page_size,
    }))
}

/// Режим грида: страница, сортировка и фильтр по схеме `platform-grid`.
async fn list_grid(state: &AppState, params: &ListParams) -> Result<Json<TenantPage>, ApiError> {
    let mut pairs: Vec<(&str, String)> = Vec::new();
    if let Some(page) = params.page {
        pairs.push(("page", page.to_string()));
    }
    if let Some(size) = params.page_size {
        pairs.push(("page_size", size.to_string()));
    }
    if let Some(sort) = &params.sort {
        pairs.push(("sort", sort.clone()));
    }
    if let Some(filter) = &params.filter {
        pairs.push(("filter", filter.clone()));
    }
    let query = GridQuery::from_pairs(pairs.iter().map(|(k, v)| (*k, v.as_str())))
        .map_err(platform_grid::GridError::from)?;
    let page = state.tenants.list_grid(query).await?;
    Ok(Json(TenantPage {
        items: page.items.into_iter().map(to_dto).collect(),
        total: i64::try_from(page.total).unwrap_or(i64::MAX),
        page: i64::from(page.page),
        page_size: i64::from(page.page_size),
    }))
}

#[utoipa::path(get, path = "/tenants/{id}", security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Tenant id")), responses(
    (status = 200, body = TenantDto),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TenantDto>, ApiError> {
    user.require(Permission::ManageTenants)?;
    Ok(Json(to_dto(state.tenants.get(TenantId(id)).await?)))
}

#[utoipa::path(post, path = "/tenants/{id}/suspend", security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Tenant id")), responses(
    (status = 200, body = TenantDto),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn suspend(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TenantDto>, ApiError> {
    user.require(Permission::ManageTenants)?;
    Ok(Json(to_dto(state.tenants.suspend(TenantId(id)).await?)))
}

#[utoipa::path(post, path = "/tenants/{id}/activate", security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Tenant id")), responses(
    (status = 200, body = TenantDto),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn activate(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TenantDto>, ApiError> {
    user.require(Permission::ManageTenants)?;
    Ok(Json(to_dto(state.tenants.activate(TenantId(id)).await?)))
}

impl From<TenantError> for ApiError {
    fn from(e: TenantError) -> Self {
        match e {
            TenantError::Domain(d @ DomainError::InvalidTransition { .. }) => {
                ApiError::conflict(d.to_string())
            }
            TenantError::Domain(d) => ApiError::bad_request(d.to_string()),
            TenantError::Invalid(m) => ApiError::bad_request(m),
            TenantError::Grid(e) => e.into(),
            TenantError::SlugTaken => ApiError::conflict("slug already taken"),
            TenantError::NotFound => ApiError::not_found(),
            TenantError::Provisioning(m) => {
                tracing::error!(error = %m, "tenant provisioning failed");
                ApiError::new(StatusCode::BAD_GATEWAY, "Bad Gateway")
                    .with_detail("identity service could not create the tenant owner")
            }
            TenantError::Db(e) => ApiError::internal(&e),
        }
    }
}
