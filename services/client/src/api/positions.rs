use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
};
use client_contracts::{PositionDto, PositionRequest};
use platform_auth::{AuthUser, Permission};
use platform_core::error::{ApiError, ProblemDetails};
use uuid::Uuid;

use super::{AppState, error::tenant_of, mapping::position_to_dto};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/positions", get(list).post(create))
        .route("/positions/{id}", patch(rename).delete(remove))
}

#[utoipa::path(get, path = "/positions", security(("bearer" = [])), responses(
    (status = 200, body = Vec<PositionDto>),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<PositionDto>>, ApiError> {
    user.require(Permission::ReadData)?;
    let rows = state.positions.list(tenant_of(&user)?).await?;
    Ok(Json(rows.iter().map(position_to_dto).collect()))
}

#[utoipa::path(post, path = "/positions", security(("bearer" = [])), request_body = PositionRequest, responses(
    (status = 201, body = PositionDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<PositionRequest>,
) -> Result<(StatusCode, Json<PositionDto>), ApiError> {
    user.require(Permission::WriteData)?;
    let row = state.positions.create(tenant_of(&user)?, &req.name).await?;
    Ok((StatusCode::CREATED, Json(position_to_dto(&row))))
}

#[utoipa::path(patch, path = "/positions/{id}", security(("bearer" = [])), request_body = PositionRequest,
    params(("id" = Uuid, Path, description = "Position id")), responses(
    (status = 200, body = PositionDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn rename(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<PositionRequest>,
) -> Result<Json<PositionDto>, ApiError> {
    user.require(Permission::WriteData)?;
    let row = state
        .positions
        .rename(tenant_of(&user)?, id, &req.name)
        .await?;
    Ok(Json(position_to_dto(&row)))
}

#[utoipa::path(delete, path = "/positions/{id}", security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Position id")), responses(
    (status = 204),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    user.require(Permission::WriteData)?;
    state.positions.delete(tenant_of(&user)?, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
