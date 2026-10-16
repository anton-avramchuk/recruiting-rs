use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
};
use platform_auth::{AuthUser, Permission, Role};
use platform_core::{
    error::{ApiError, ProblemDetails},
    tenant::TenantId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AppState;
use crate::{
    application::users::{NewUser, UserChanges, UserError},
    domain::{User, UserStatus},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users", get(list).post(create))
        .route("/users/{id}", patch(update))
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct UserDto {
    id: Uuid,
    email: String,
    display_name: String,
    roles: Vec<Role>,
    status: &'static str,
}

impl From<User> for UserDto {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            email: u.email.as_str().to_string(),
            display_name: u.display_name,
            roles: u.roles,
            status: u.status.as_str(),
        }
    }
}

/// Тенант берётся только из токена, никогда из запроса.
fn tenant_of(user: &AuthUser) -> Result<TenantId, ApiError> {
    user.claims.tid.ok_or_else(ApiError::forbidden)
}

#[utoipa::path(get, path = "/users", security(("bearer" = [])), responses(
    (status = 200, body = Vec<UserDto>),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<UserDto>>, ApiError> {
    user.require(Permission::ManageUsers)?;
    let users = state.users.list(tenant_of(&user)?).await?;
    Ok(Json(users.into_iter().map(Into::into).collect()))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct CreateUserRequest {
    email: String,
    password: String,
    #[serde(default)]
    display_name: String,
    roles: Vec<Role>,
}

#[utoipa::path(post, path = "/users", security(("bearer" = [])), request_body = CreateUserRequest, responses(
    (status = 201, body = UserDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserDto>), ApiError> {
    user.require(Permission::ManageUsers)?;
    let created = state
        .users
        .create(
            tenant_of(&user)?,
            NewUser {
                email: req.email,
                password: req.password,
                display_name: req.display_name,
                roles: req.roles,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct UpdateUserRequest {
    roles: Option<Vec<Role>>,
    status: Option<String>,
    display_name: Option<String>,
    /// Новый пароль (сброс владельцем).
    password: Option<String>,
}

#[utoipa::path(patch, path = "/users/{id}", security(("bearer" = [])), request_body = UpdateUserRequest,
    params(("id" = Uuid, Path, description = "User id")), responses(
    (status = 200, body = UserDto),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<UserDto>, ApiError> {
    user.require(Permission::ManageUsers)?;
    let status = req
        .status
        .map(|s| UserStatus::parse(&s).ok_or_else(|| ApiError::bad_request("unknown status")))
        .transpose()?;
    let updated = state
        .users
        .update(
            tenant_of(&user)?,
            user.claims.sub,
            id,
            UserChanges {
                roles: req.roles,
                status,
                display_name: req.display_name,
                password: req.password,
            },
        )
        .await?;
    Ok(Json(updated.into()))
}

impl From<UserError> for ApiError {
    fn from(e: UserError) -> Self {
        match e {
            UserError::Domain(d) => ApiError::bad_request(d.to_string()),
            UserError::Conflict(m) => ApiError::conflict(m),
            UserError::NotFound => ApiError::not_found(),
            UserError::Db(e) => ApiError::internal(&e),
            UserError::Internal(m) => ApiError::internal(&std::io::Error::other(m)),
        }
    }
}
