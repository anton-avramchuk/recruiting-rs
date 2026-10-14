//! HTTP-слой admin.
mod auth;
mod internal;
mod openapi;
mod state;
mod tenants;

use axum::Router;

pub use state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(auth::routes())
        .merge(tenants::routes())
        .merge(internal::routes())
        .merge(openapi::routes())
        .merge(platform_core::health::router())
        .merge(platform_auth::jwks::router(&state.public_key))
        .with_state(state)
}
