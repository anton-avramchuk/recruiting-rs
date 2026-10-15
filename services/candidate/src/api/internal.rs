//! Эндпоинты для других сервисов платформы (защищены сервисным токеном).
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use candidate_contracts::{PersonChange, PersonChangesPage};
use platform_auth::ServiceCaller;
use platform_core::{
    error::{ApiError, ProblemDetails},
    tenant::TenantId,
};
use serde::Deserialize;
use uuid::Uuid;

use super::AppState;

const DEFAULT_LIMIT: i64 = 500;
const MAX_LIMIT: i64 = 1000;

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/internal/tenants/{tenant_id}/person-changes",
        get(person_changes),
    )
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct ChangesParams {
    /// Позиция, после которой читать (по умолчанию 0 - с начала).
    after: Option<i64>,
    limit: Option<i64>,
}

#[utoipa::path(get, path = "/internal/tenants/{tenant_id}/person-changes", security(("service" = [])),
    params(("tenant_id" = Uuid, Path, description = "Tenant id"), ChangesParams), responses(
    (status = 200, body = PersonChangesPage),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn person_changes(
    _caller: ServiceCaller,
    State(state): State<AppState>,
    Path(tenant_id): Path<Uuid>,
    Query(params): Query<ChangesParams>,
) -> Result<Json<PersonChangesPage>, ApiError> {
    let after = params.after.unwrap_or(0);
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT);
    if after < 0 || limit < 1 {
        return Err(ApiError::bad_request(
            "after must not be negative and limit must be positive",
        ));
    }
    let rows = state
        .persons
        .changes(TenantId(tenant_id), after, limit.min(MAX_LIMIT))
        .await
        .map_err(|e| ApiError::internal(&e))?;
    let next = rows.last().map_or(after, |r| r.seq);
    let items = rows
        .into_iter()
        .map(|r| PersonChange {
            seq: r.seq,
            person_id: r.person_id,
            deleted: r.deleted,
            snapshot: r.snapshot.and_then(|s| serde_json::from_value(s).ok()),
        })
        .collect();
    Ok(Json(PersonChangesPage { items, next }))
}
