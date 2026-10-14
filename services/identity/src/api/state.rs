use std::sync::Arc;

use axum::extract::FromRef;
use platform_auth::{Audience, KeyPair, PublicKey, TokenSigner, TokenVerifier};
use sqlx::PgPool;

use crate::application::{
    auth::{AuthService, TokenLifetimes},
    tenants::TenantDirectory,
    users::UserService,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth: AuthService,
    pub users: UserService,
    pub verifier: Arc<TokenVerifier>,
    pub public_key: PublicKey,
}

impl AppState {
    pub fn new(pool: PgPool, directory: Arc<dyn TenantDirectory>, keys: &KeyPair) -> Self {
        let signer = Arc::new(TokenSigner::new(keys));
        let public_key = keys.public_key();
        Self {
            users: UserService::new(pool.clone()),
            auth: AuthService::new(pool.clone(), directory, signer, TokenLifetimes::default()),
            verifier: Arc::new(TokenVerifier::new(&public_key, Audience::Tenant)),
            public_key,
            pool,
        }
    }
}

impl FromRef<AppState> for Arc<TokenVerifier> {
    fn from_ref(state: &AppState) -> Self {
        state.verifier.clone()
    }
}
