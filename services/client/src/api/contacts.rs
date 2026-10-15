use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::patch,
};
use client_contracts::{ContactDto, UpdateContactRequest};
use platform_auth::{AuthUser, Permission};
use platform_core::error::{ApiError, ProblemDetails};
use uuid::Uuid;

use super::{AppState, error::tenant_of, mapping::contact_to_dto};
use crate::domain::ContactPatch;

pub fn routes() -> Router<AppState> {
    Router::new().route("/contacts/{id}", patch(update).delete(remove))
}

#[utoipa::path(patch, path = "/contacts/{id}", security(("bearer" = [])), request_body = UpdateContactRequest,
    params(("id" = Uuid, Path, description = "Contact id")), responses(
    (status = 200, body = ContactDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateContactRequest>,
) -> Result<Json<ContactDto>, ApiError> {
    user.require(Permission::WriteData)?;
    let row = state
        .clients
        .update_contact(
            tenant_of(&user)?,
            id,
            ContactPatch {
                full_name: req.full_name,
                position_id: req.position_id,
                email: req.email,
                phone: req.phone,
                note: req.note,
            },
        )
        .await?;
    Ok(Json(contact_to_dto(&row)))
}

#[utoipa::path(delete, path = "/contacts/{id}", security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Contact id")), responses(
    (status = 204),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    user.require(Permission::WriteData)?;
    state.clients.delete_contact(tenant_of(&user)?, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
