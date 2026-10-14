//! Справочник тенантов для других сервисов (защищён сервисным токеном).
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use platform_auth::ServiceCaller;
use platform_core::{error::ApiError, tenant::TenantId};
use uuid::Uuid;

use super::{AppState, tenants::TenantDto};
use crate::infrastructure::tenants as repo;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/internal/tenants/by-slug/{slug}", get(by_slug))
        .route("/internal/tenants/{id}", get(by_id))
}

async fn by_slug(
    _caller: ServiceCaller,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<TenantDto>, ApiError> {
    repo::find_by_slug(&state.pool, &slug)
        .await
        .map_err(|e| ApiError::internal(&e))?
        .map(|t| Json(t.into()))
        .ok_or_else(ApiError::not_found)
}

async fn by_id(
    _caller: ServiceCaller,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TenantDto>, ApiError> {
    repo::find_by_id(&state.pool, TenantId(id))
        .await
        .map_err(|e| ApiError::internal(&e))?
        .map(|t| Json(t.into()))
        .ok_or_else(ApiError::not_found)
}
