use axum::{Json, Router, extract::State, routing::post};
use platform_core::error::ApiError;
use serde::Deserialize;

use super::AppState;
use crate::application::auth::TokenPair;

pub fn routes() -> Router<AppState> {
    Router::new().route("/auth/login", post(login))
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
