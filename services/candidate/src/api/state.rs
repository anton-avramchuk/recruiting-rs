use std::sync::Arc;

use axum::extract::FromRef;
use chrono::NaiveDate;
use platform_auth::TokenVerifier;
use sqlx::PgPool;

use crate::application::{PersonService, persons::Clock};

#[derive(Clone)]
pub struct AppState {
    pub persons: PersonService,
    pub verifier: Arc<TokenVerifier>,
    pool: PgPool,
}

impl AppState {
    pub fn new(pool: PgPool, verifier: Arc<TokenVerifier>) -> Self {
        Self {
            persons: PersonService::new(pool.clone(), Clock::System),
            verifier,
            pool,
        }
    }

    /// Фиксирует «сегодня» (для тестов).
    pub fn with_today(mut self, today: NaiveDate) -> Self {
        self.persons = PersonService::new(self.pool.clone(), Clock::Fixed(today));
        self
    }
}

impl FromRef<AppState> for Arc<TokenVerifier> {
    fn from_ref(state: &AppState) -> Self {
        state.verifier.clone()
    }
}
