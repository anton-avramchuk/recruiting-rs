//! Справочник тенантов для других сервисов (защищён сервисным токеном).
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use platform_auth::ServiceCaller;
use platform_core::{
    error::{ApiError, ProblemDetails},
    tenant::TenantId,
};
use uuid::Uuid;

use admin_contracts::TenantDto;

use super::{AppState, tenants::to_dto};
use crate::infrastructure::tenants as repo;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/internal/tenants/by-slug/{slug}", get(by_slug))
        .route("/internal/tenants/{id}", get(by_id))
}

#[utoipa::path(get, path = "/internal/tenants/by-slug/{slug}", security(("service" = [])),
    params(("slug" = String, Path, description = "Tenant slug")), responses(
    (status = 200, body = TenantDto),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn by_slug(
    _caller: ServiceCaller,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<TenantDto>, ApiError> {
    repo::find_by_slug(&state.pool, &slug)
        .await
        .map_err(|e| ApiError::internal(&e))?
        .map(|t| Json(to_dto(t)))
        .ok_or_else(ApiError::not_found)
}

#[utoipa::path(get, path = "/internal/tenants/{id}", security(("service" = [])),
    params(("id" = Uuid, Path, description = "Tenant id")), responses(
    (status = 200, body = TenantDto),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn by_id(
    _caller: ServiceCaller,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TenantDto>, ApiError> {
    repo::find_by_id(&state.pool, TenantId(id))
        .await
        .map_err(|e| ApiError::internal(&e))?
        .map(|t| Json(to_dto(t)))
        .ok_or_else(ApiError::not_found)
}
