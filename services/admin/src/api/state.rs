use std::sync::Arc;

use axum::extract::FromRef;
use platform_auth::{Audience, KeyPair, PublicKey, ServiceToken, TokenSigner, TokenVerifier};
use sqlx::PgPool;

use crate::application::{provisioning::Provisioner, tenants::TenantService};

/// Срок жизни токена администратора. Refresh не предусмотрен: по истечении BFF просит войти заново.
pub const ADMIN_TOKEN_TTL_SECS: i64 = 8 * 3600;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub tenants: TenantService,
    pub signer: Arc<TokenSigner>,
    pub verifier: Arc<TokenVerifier>,
    pub service_token: Arc<ServiceToken>,
    pub public_key: PublicKey,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        keys: &KeyPair,
        service_token: impl Into<String>,
        provisioner: Arc<dyn Provisioner>,
    ) -> Self {
        let public_key = keys.public_key();
        Self {
            tenants: TenantService::new(pool.clone(), provisioner),
            pool,
            signer: Arc::new(TokenSigner::new(keys)),
            verifier: Arc::new(TokenVerifier::new(&public_key, Audience::Platform)),
            service_token: Arc::new(ServiceToken::new(service_token)),
            public_key,
        }
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
