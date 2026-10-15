//! HTTP-слой recruitment.
mod analytics;
mod applications;
mod error;
mod mapping;
mod openapi;
mod state;
mod vacancies;

use axum::Router;

pub use state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(vacancies::routes())
        .merge(applications::routes())
        .merge(analytics::routes())
        .merge(openapi::routes())
        .merge(platform_core::health::router())
        .with_state(state)
}
