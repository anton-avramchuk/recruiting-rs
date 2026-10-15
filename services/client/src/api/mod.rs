//! HTTP-слой client.
mod clients;
mod contacts;
mod error;
mod internal;
mod mapping;
mod openapi;
mod positions;
mod state;

use axum::Router;

pub use state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(clients::routes())
        .merge(contacts::routes())
        .merge(positions::routes())
        .merge(internal::routes())
        .merge(openapi::routes())
        .merge(platform_core::health::router())
        .with_state(state)
}
