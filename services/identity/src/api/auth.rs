use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use platform_core::error::{ApiError, ProblemDetails};
use serde::Deserialize;

use super::AppState;
use crate::application::auth::TokenPair;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct LoginRequest {
    tenant: String,
    email: String,
    password: String,
}

#[utoipa::path(post, path = "/auth/login", request_body = LoginRequest, responses(
    (status = 200, body = TokenPair),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<TokenPair>, ApiError> {
    let pair = state
        .auth
        .login(&req.tenant, &req.email, &req.password)
        .await?;
    Ok(Json(pair))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct RefreshRequest {
    refresh_token: String,
}

#[utoipa::path(post, path = "/auth/refresh", request_body = RefreshRequest, responses(
    (status = 200, body = TokenPair),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<TokenPair>, ApiError> {
    Ok(Json(state.auth.refresh(&req.refresh_token).await?))
}

#[utoipa::path(post, path = "/auth/logout", request_body = RefreshRequest, responses(
    (status = 204, description = "Refresh token revoked"),
))]
pub(super) async fn logout(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<StatusCode, ApiError> {
    state.auth.logout(&req.refresh_token).await?;
    Ok(StatusCode::NO_CONTENT)
}
