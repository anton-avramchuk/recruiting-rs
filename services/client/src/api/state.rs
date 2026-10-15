use std::sync::Arc;

use axum::extract::FromRef;
use platform_auth::{ServiceToken, TokenVerifier};
use sqlx::PgPool;

use crate::application::{ClientService, FeedService, PositionService};

#[derive(Clone)]
pub struct AppState {
    pub clients: ClientService,
    pub positions: PositionService,
    pub feed: FeedService,
    pub verifier: Arc<TokenVerifier>,
    pub service_token: Arc<ServiceToken>,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        verifier: Arc<TokenVerifier>,
        service_token: impl Into<String>,
    ) -> Self {
        Self {
            clients: ClientService::new(pool.clone()),
            positions: PositionService::new(pool.clone()),
            feed: FeedService::new(pool),
            verifier,
            service_token: Arc::new(ServiceToken::new(service_token)),
        }
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
