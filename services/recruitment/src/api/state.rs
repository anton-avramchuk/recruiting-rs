use std::sync::Arc;

use axum::extract::FromRef;
use platform_auth::TokenVerifier;
use sqlx::PgPool;

use crate::application::{
    AnalyticsService, ApplicationService, ClientFeed, ClientsSync, Clock, PeopleSync, PersonFeed,
    VacancyService,
};

#[derive(Clone)]
pub struct AppState {
    pub vacancies: VacancyService,
    pub applications: ApplicationService,
    pub analytics: AnalyticsService,
    pub verifier: Arc<TokenVerifier>,
    pool: PgPool,
    feed: Arc<dyn PersonFeed>,
    client_feed: Arc<dyn ClientFeed>,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        verifier: Arc<TokenVerifier>,
        feed: Arc<dyn PersonFeed>,
        client_feed: Arc<dyn ClientFeed>,
    ) -> Self {
        Self::build(pool, verifier, feed, client_feed, Clock::System)
    }

    /// Фиксирует «сейчас» (для тестов).
    pub fn with_clock(self, clock: Clock) -> Self {
        Self::build(self.pool, self.verifier, self.feed, self.client_feed, clock)
    }

    fn build(
        pool: PgPool,
        verifier: Arc<TokenVerifier>,
        feed: Arc<dyn PersonFeed>,
        client_feed: Arc<dyn ClientFeed>,
        clock: Clock,
    ) -> Self {
        let sync = PeopleSync::new(pool.clone(), feed.clone());
        let clients_sync = ClientsSync::new(pool.clone(), client_feed.clone());
        Self {
            vacancies: VacancyService::new(pool.clone(), clients_sync, clock),
            applications: ApplicationService::new(pool.clone(), sync.clone(), clock),
            analytics: AnalyticsService::new(pool.clone(), sync, clock),
            verifier,
            pool,
            feed,
            client_feed,
        }
    }
}

impl FromRef<AppState> for Arc<TokenVerifier> {
    fn from_ref(state: &AppState) -> Self {
        state.verifier.clone()
    }
}
