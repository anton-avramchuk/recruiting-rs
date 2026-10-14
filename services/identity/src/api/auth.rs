use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use platform_core::error::ApiError;
use serde::Deserialize;

use super::AppState;
use crate::application::auth::TokenPair;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
}

#[derive(Deserialize)]
struct LoginRequest {
    tenant: String,
    email: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<TokenPair>, ApiError> {
    let pair = state
        .auth
        .login(&req.tenant, &req.email, &req.password)
        .await?;
    Ok(Json(pair))
}

#[derive(Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<TokenPair>, ApiError> {
    Ok(Json(state.auth.refresh(&req.refresh_token).await?))
}

async fn logout(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<StatusCode, ApiError> {
    state.auth.logout(&req.refresh_token).await?;
    Ok(StatusCode::NO_CONTENT)
}
