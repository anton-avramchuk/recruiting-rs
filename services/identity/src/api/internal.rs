//! Эндпоинты для других сервисов платформы (защищены сервисным токеном).
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use platform_auth::{Role, ServiceCaller};
use platform_core::{
    error::{ApiError, ProblemDetails},
    tenant::TenantId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AppState;
use crate::application::users::NewUser;

pub fn routes() -> Router<AppState> {
    Router::new().route("/internal/tenants/{tenant_id}/owner", post(ensure_owner))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct OwnerRequest {
    email: String,
    password: String,
    #[serde(default)]
    display_name: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct OwnerDto {
    id: Uuid,
    email: String,
    roles: Vec<Role>,
}

#[utoipa::path(post, path = "/internal/tenants/{tenant_id}/owner", security(("service" = [])),
    request_body = OwnerRequest, params(("tenant_id" = Uuid, Path, description = "Tenant id")), responses(
    (status = 201, body = OwnerDto),
    (status = 200, body = OwnerDto, description = "Owner already existed"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn ensure_owner(
    _caller: ServiceCaller,
    State(state): State<AppState>,
    Path(tenant_id): Path<Uuid>,
    Json(req): Json<OwnerRequest>,
) -> Result<(StatusCode, Json<OwnerDto>), ApiError> {
    let (user, created) = state
        .users
        .ensure_owner(
            TenantId(tenant_id),
            NewUser {
                email: req.email,
                password: req.password,
                display_name: req.display_name,
                roles: vec![Role::Owner],
            },
        )
        .await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(OwnerDto {
            id: user.id,
            email: user.email.as_str().to_string(),
            roles: user.roles,
        }),
    ))
}
