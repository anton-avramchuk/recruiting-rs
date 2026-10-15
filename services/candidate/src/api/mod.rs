//! HTTP-слой candidate.
mod internal;
mod mapping;
mod openapi;
mod persons;
mod state;

use axum::Router;

pub use state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(persons::routes())
        .merge(internal::routes())
        .merge(openapi::routes())
        .merge(platform_core::health::router())
        .with_state(state)
}
