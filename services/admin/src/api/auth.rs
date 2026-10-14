use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use platform_auth::{Audience, AuthUser, Claims, Permission, Role, password};
use platform_core::error::{ApiError, ProblemDetails};
use serde::{Deserialize, Serialize};

use super::{AppState, state::ADMIN_TOKEN_TTL_SECS};
use crate::infrastructure::admins;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/me", get(me))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct TokenResponse {
    access_token: String,
    expires_in: i64,
}

#[utoipa::path(post, path = "/auth/login", request_body = LoginRequest, responses(
    (status = 200, body = TokenResponse),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<TokenResponse>, ApiError> {
    let admin = admins::find_by_email(&state.pool, &req.email)
        .await
        .map_err(|e| ApiError::internal(&e))?;
    let Some(admin) = admin.filter(|a| password::verify(&req.password, &a.password_hash)) else {
        password::verify_dummy(&req.password);
        return Err(ApiError::unauthorized());
    };
    let claims = Claims::new(
        admin.id,
        None,
        vec![Role::PlatformAdmin],
        Audience::Platform,
        ADMIN_TOKEN_TTL_SECS,
    );
    let access_token = state
        .signer
        .sign(&claims)
        .map_err(|e| ApiError::internal(&e))?;
    Ok(Json(TokenResponse {
        access_token,
        expires_in: ADMIN_TOKEN_TTL_SECS,
    }))
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct MeResponse {
    id: uuid::Uuid,
    email: String,
}

#[utoipa::path(get, path = "/me", security(("bearer" = [])), responses(
    (status = 200, body = MeResponse),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn me(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<MeResponse>, ApiError> {
    user.require(Permission::ManageTenants)?;
    let admin = admins::find_by_id(&state.pool, user.claims.sub)
        .await
        .map_err(|e| ApiError::internal(&e))?
        .ok_or_else(ApiError::unauthorized)?;
    Ok(Json(MeResponse {
        id: admin.id,
        email: admin.email,
    }))
}
