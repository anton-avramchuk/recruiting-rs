use std::sync::Arc;

use axum::extract::FromRef;
use chrono::NaiveDate;
use platform_auth::{ServiceToken, TokenVerifier};
use sqlx::PgPool;

use crate::application::{PersonService, persons::Clock};

#[derive(Clone)]
pub struct AppState {
    pub persons: PersonService,
    pub verifier: Arc<TokenVerifier>,
    pub service_token: Arc<ServiceToken>,
    pool: PgPool,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        verifier: Arc<TokenVerifier>,
        service_token: impl Into<String>,
    ) -> Self {
        Self {
            persons: PersonService::new(pool.clone(), Clock::System),
            verifier,
            service_token: Arc::new(ServiceToken::new(service_token)),
            pool,
        }
    }

    /// Фиксирует «сегодня» (для тестов).
    pub fn with_today(mut self, today: NaiveDate) -> Self {
        self.persons = PersonService::new(self.pool.clone(), Clock::Fixed(today));
        self
    }
}

impl FromRef<AppState> for Arc<ServiceToken> {
    fn from_ref(state: &AppState) -> Self {
        state.service_token.clone()
    }
}

impl FromRef<AppState> for Arc<TokenVerifier> {
    fn from_ref(state: &AppState) -> Self {
        state.verifier.clone()
    }
}
