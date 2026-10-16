use std::sync::Arc;

use axum::extract::FromRef;
use platform_auth::{Audience, KeyPair, PublicKey, ServiceToken, TokenSigner, TokenVerifier};
use sqlx::PgPool;

use crate::application::{
    auth::{AuthService, TokenLifetimes},
    roles::RoleService,
    tenants::TenantDirectory,
    users::UserService,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth: AuthService,
    pub users: UserService,
    pub roles: RoleService,
    pub verifier: Arc<TokenVerifier>,
    pub service_token: Arc<ServiceToken>,
    pub public_key: PublicKey,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        directory: Arc<dyn TenantDirectory>,
        keys: &KeyPair,
        service_token: impl Into<String>,
    ) -> Self {
        let signer = Arc::new(TokenSigner::new(keys));
        let public_key = keys.public_key();
        Self {
            users: UserService::new(pool.clone()),
            roles: RoleService::new(pool.clone()),
            auth: AuthService::new(pool.clone(), directory, signer, TokenLifetimes::default()),
            verifier: Arc::new(TokenVerifier::new(&public_key, Audience::Tenant)),
            service_token: Arc::new(ServiceToken::new(service_token)),
            public_key,
            pool,
        }
    }
}

impl AppState {
    /// Переопределяет сроки жизни токенов (по умолчанию: access 10 минут, refresh 30 дней).
    pub fn with_lifetimes(mut self, lifetimes: TokenLifetimes) -> Self {
        self.auth.lifetimes = lifetimes;
        self
    }
}

impl FromRef<AppState> for Arc<TokenVerifier> {
    fn from_ref(state: &AppState) -> Self {
        state.verifier.clone()
    }
}

impl FromRef<AppState> for Arc<ServiceToken> {
    fn from_ref(state: &AppState) -> Self {
        state.service_token.clone()
    }
}
