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
use serde::{Deserialize, Serialize};
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

#[derive(Serialize, utoipa::ToSchema)]
pub struct TenantDto {
    id: Uuid,
    slug: String,
    name: String,
    status: &'static str,
}

impl From<Tenant> for TenantDto {
    fn from(t: Tenant) -> Self {
        Self {
            id: t.id.0,
            slug: t.slug.as_str().to_string(),
            name: t.name,
            status: t.status.as_str(),
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct OwnerRequest {
    email: String,
    password: String,
    #[serde(default)]
    display_name: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct CreateTenantRequest {
    slug: String,
    name: String,
    owner: OwnerRequest,
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
    Ok((StatusCode::CREATED, Json(tenant.into())))
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct ListParams {
    status: Option<String>,
    q: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct PageDto {
    items: Vec<TenantDto>,
    total: i64,
    page: i64,
    page_size: i64,
}

#[utoipa::path(get, path = "/tenants", security(("bearer" = [])), params(ListParams), responses(
    (status = 200, body = PageDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<ListParams>,
) -> Result<Json<PageDto>, ApiError> {
    user.require(Permission::ManageTenants)?;
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
    Ok(Json(PageDto {
        items: page.items.into_iter().map(Into::into).collect(),
        total: page.total,
        page: page.page,
        page_size: page.page_size,
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
    Ok(Json(state.tenants.get(TenantId(id)).await?.into()))
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
    Ok(Json(state.tenants.suspend(TenantId(id)).await?.into()))
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
    Ok(Json(state.tenants.activate(TenantId(id)).await?.into()))
}

impl From<TenantError> for ApiError {
    fn from(e: TenantError) -> Self {
        match e {
            TenantError::Domain(d @ DomainError::InvalidTransition { .. }) => {
                ApiError::conflict(d.to_string())
            }
            TenantError::Domain(d) => ApiError::bad_request(d.to_string()),
            TenantError::Invalid(m) => ApiError::bad_request(m),
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
