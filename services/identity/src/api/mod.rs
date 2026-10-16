//! HTTP-слой identity.
mod auth;
mod error;
mod internal;
mod openapi;
mod roles;
mod state;
mod users;

use axum::Router;

pub use state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(auth::routes())
        .merge(users::routes())
        .merge(roles::routes())
        .merge(internal::routes())
        .merge(openapi::routes())
        .merge(platform_core::health::router())
        .merge(platform_auth::jwks::router(&state.public_key))
        .with_state(state)
}
