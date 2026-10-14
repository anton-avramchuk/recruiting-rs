use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
};
use platform_auth::{AuthUser, Permission, Role};
use platform_core::{error::ApiError, tenant::TenantId};
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

#[derive(Serialize)]
struct UserDto {
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

async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<UserDto>>, ApiError> {
    user.require(Permission::ManageUsers)?;
    let users = state.users.list(tenant_of(&user)?).await?;
    Ok(Json(users.into_iter().map(Into::into).collect()))
}

#[derive(Deserialize)]
struct CreateUserRequest {
    email: String,
    password: String,
    #[serde(default)]
    display_name: String,
    roles: Vec<Role>,
}

async fn create(
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

#[derive(Deserialize)]
struct UpdateUserRequest {
    roles: Option<Vec<Role>>,
    status: Option<String>,
    display_name: Option<String>,
}

async fn update(
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
