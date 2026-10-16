//! HTTP-слой comment.
mod comments;
mod error;
mod mapping;
mod openapi;
mod state;

use axum::Router;

pub use state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(comments::routes())
        .merge(openapi::routes())
        .merge(platform_core::health::router())
        .with_state(state)
}
