use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use grid_contracts::{GridPage, GridQuery};
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
        .route("/users/{id}", get(get_one).patch(update))
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct UserDto {
    id: Uuid,
    email: String,
    display_name: String,
    roles: Vec<Role>,
    /// Собственная роль организации, если назначена.
    role_id: Option<Uuid>,
    role_name: Option<String>,
    status: &'static str,
    /// RFC 3339; пусто у только что созданного пользователя.
    created_at: Option<String>,
}

impl From<User> for UserDto {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            email: u.email.as_str().to_string(),
            display_name: u.display_name,
            roles: u.roles,
            role_id: u.role_id,
            role_name: u.role_name,
            status: u.status.as_str(),
            created_at: u.created_at.map(|t| t.to_rfc3339()),
        }
    }
}

/// Тенант берётся только из токена, никогда из запроса.
fn tenant_of(user: &AuthUser) -> Result<TenantId, ApiError> {
    user.claims.tid.ok_or_else(ApiError::forbidden)
}

/// Параметры списка: `page`, `page_size`, `sort` (`поле:asc,поле:desc`), `filter` (JSON).
#[utoipa::path(get, path = "/users", security(("bearer" = [])), params(
    ("page" = Option<u32>, Query, description = "Страница, с 1"),
    ("page_size" = Option<u32>, Query, description = "Размер страницы, до 100"),
    ("sort" = Option<String>, Query, description = "Сортировка: поле:asc,поле:desc"),
    ("filter" = Option<String>, Query, description = "Фильтр грида (JSON)"),
), responses(
    (status = 200, body = GridPage<UserDto>),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<Vec<(String, String)>>,
) -> Result<Json<GridPage<UserDto>>, ApiError> {
    user.require(Permission::ManageUsers)?;
    let query = GridQuery::from_pairs(params.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .map_err(platform_grid::GridError::from)?;
    let page = state.users.list(tenant_of(&user)?, query).await?;
    Ok(Json(GridPage {
        items: page.items.into_iter().map(Into::into).collect(),
        total: page.total,
        page: page.page,
        page_size: page.page_size,
    }))
}

#[utoipa::path(get, path = "/users/{id}", security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "User id")), responses(
    (status = 200, body = UserDto),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<UserDto>, ApiError> {
    user.require(Permission::ManageUsers)?;
    Ok(Json(state.users.get(tenant_of(&user)?, id).await?.into()))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct CreateUserRequest {
    email: String,
    password: String,
    #[serde(default)]
    display_name: String,
    /// Встроенные роли либо `role_id` (одно из двух).
    #[serde(default)]
    roles: Vec<Role>,
    role_id: Option<Uuid>,
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
                role_id: req.role_id,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct UpdateUserRequest {
    roles: Option<Vec<Role>>,
    role_id: Option<Uuid>,
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
                role_id: req.role_id,
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
            UserError::Grid(e) => e.into(),
        }
    }
}
