use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
};
use platform_auth::{AuthUser, Permission};
use platform_core::{
    error::{ApiError, ProblemDetails},
    tenant::TenantId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AppState;
use crate::application::roles::{RoleChanges, RoleError, RoleView};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/roles", get(list).post(create))
        .route("/roles/{id}", patch(update).delete(remove))
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct RoleDto {
    /// Код встроенной роли либо идентификатор собственной.
    id: String,
    /// `builtin` или `custom`.
    kind: &'static str,
    name: String,
    permissions: Vec<&'static str>,
    /// Сколько пользователей имеют эту роль.
    users: i64,
}

impl From<RoleView> for RoleDto {
    fn from(r: RoleView) -> Self {
        Self {
            id: r.id,
            kind: if r.custom { "custom" } else { "builtin" },
            name: r.name,
            permissions: r.permissions.iter().map(|p| p.as_str()).collect(),
            users: r.users,
        }
    }
}

fn tenant_of(user: &AuthUser) -> Result<TenantId, ApiError> {
    user.claims.tid.ok_or_else(ApiError::forbidden)
}

/// Идентификатор собственной роли; всё остальное (в том числе коды встроенных ролей) - 404.
fn custom_id(raw: &str) -> Result<Uuid, ApiError> {
    raw.parse().map_err(|_| ApiError::not_found())
}

#[utoipa::path(get, path = "/roles", security(("bearer" = [])), responses(
    (status = 200, body = Vec<RoleDto>),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<RoleDto>>, ApiError> {
    user.require(Permission::ManageUsers)?;
    let roles = state.roles.list(tenant_of(&user)?).await?;
    Ok(Json(roles.into_iter().map(Into::into).collect()))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct CreateRoleRequest {
    name: String,
    permissions: Vec<String>,
}

#[utoipa::path(post, path = "/roles", security(("bearer" = [])), request_body = CreateRoleRequest, responses(
    (status = 201, body = RoleDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateRoleRequest>,
) -> Result<(StatusCode, Json<RoleDto>), ApiError> {
    user.require(Permission::ManageUsers)?;
    let role = state
        .roles
        .create(tenant_of(&user)?, &req.name, &req.permissions)
        .await?;
    Ok((StatusCode::CREATED, Json(role.into())))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct UpdateRoleRequest {
    name: Option<String>,
    permissions: Option<Vec<String>>,
}

#[utoipa::path(patch, path = "/roles/{id}", security(("bearer" = [])), request_body = UpdateRoleRequest,
    params(("id" = String, Path, description = "Role id")), responses(
    (status = 200, body = RoleDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateRoleRequest>,
) -> Result<Json<RoleDto>, ApiError> {
    user.require(Permission::ManageUsers)?;
    let role = state
        .roles
        .update(
            tenant_of(&user)?,
            custom_id(&id)?,
            RoleChanges {
                name: req.name,
                permissions: req.permissions,
            },
        )
        .await?;
    Ok(Json(role.into()))
}

#[utoipa::path(delete, path = "/roles/{id}", security(("bearer" = [])),
    params(("id" = String, Path, description = "Role id")), responses(
    (status = 204, description = "Deleted"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    user.require(Permission::ManageUsers)?;
    state
        .roles
        .delete(tenant_of(&user)?, custom_id(&id)?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

impl From<RoleError> for ApiError {
    fn from(e: RoleError) -> Self {
        match e {
            RoleError::Domain(d) => ApiError::bad_request(d.to_string()),
            RoleError::Conflict(m) => ApiError::conflict(m),
            RoleError::NotFound => ApiError::not_found(),
            RoleError::Db(e) => ApiError::internal(&e),
        }
    }
}
