use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch},
};
use comment_contracts::{CommentDto, CommentPage, CreateCommentRequest, UpdateCommentRequest};
use platform_auth::{AuthUser, Permission};
use platform_core::error::{ApiError, ProblemDetails};
use serde::Deserialize;
use uuid::Uuid;

use super::{AppState, error::tenant_of, mapping::to_dto};
use crate::domain::NewComment;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/comments", get(list).post(create))
        .route("/comments/{id}", patch(update).delete(remove))
}

/// Чужие комментарии удаляет тот, у кого есть право удаления данных (владелец и собственные роли с ним).
fn can_remove_any(user: &AuthUser) -> bool {
    user.claims.has(Permission::DeleteData)
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct ListParams {
    /// `client`, `vacancy` или `person`.
    target_type: String,
    target_id: Uuid,
    /// Курсор из `next` предыдущей страницы.
    after: Option<String>,
    limit: Option<i64>,
}

#[utoipa::path(post, path = "/comments", security(("bearer" = [])), request_body = CreateCommentRequest, responses(
    (status = 201, body = CommentDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateCommentRequest>,
) -> Result<(StatusCode, Json<CommentDto>), ApiError> {
    user.require(Permission::WriteData)?;
    let row = state
        .comments
        .create(
            tenant_of(&user)?,
            user.claims.sub,
            user.claims.name.as_deref(),
            NewComment {
                target_type: req.target_type,
                target_id: req.target_id,
                body: req.body,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(to_dto(&row))))
}

#[utoipa::path(get, path = "/comments", security(("bearer" = [])), params(ListParams), responses(
    (status = 200, body = CommentPage),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<ListParams>,
) -> Result<Json<CommentPage>, ApiError> {
    user.require(Permission::ReadData)?;
    let page = state
        .comments
        .list(
            tenant_of(&user)?,
            &params.target_type,
            params.target_id,
            params.after.as_deref(),
            params.limit,
        )
        .await?;
    Ok(Json(CommentPage {
        items: page.items.iter().map(to_dto).collect(),
        next: page.next,
    }))
}

#[utoipa::path(patch, path = "/comments/{id}", security(("bearer" = [])), request_body = UpdateCommentRequest,
    params(("id" = Uuid, Path, description = "Comment id")), responses(
    (status = 200, body = CommentDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateCommentRequest>,
) -> Result<Json<CommentDto>, ApiError> {
    user.require(Permission::WriteData)?;
    let row = state
        .comments
        .edit(tenant_of(&user)?, user.claims.sub, id, &req.body)
        .await?;
    Ok(Json(to_dto(&row)))
}

#[utoipa::path(delete, path = "/comments/{id}", security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "Comment id")), responses(
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
    state
        .comments
        .delete(
            tenant_of(&user)?,
            user.claims.sub,
            can_remove_any(&user),
            id,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
