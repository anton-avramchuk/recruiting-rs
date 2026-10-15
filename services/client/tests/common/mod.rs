#![allow(dead_code)]
use std::sync::Arc;

use axum::{Router, body::Body, http::Request};
use client::{
    MIGRATOR,
    api::{self, AppState},
};
use http_body_util::BodyExt;
use platform_auth::{Audience, Claims, KeyPair, Role, TokenSigner, TokenVerifier};
use platform_core::tenant::TenantId;
use platform_db::testing::TestDb;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

pub const SERVICE_TOKEN: &str = "test-service-token";

pub struct App {
    pub db: TestDb,
    keys: KeyPair,
    router: Router,
}

pub async fn setup() -> App {
    let db = TestDb::create(&MIGRATOR).await;
    let keys = KeyPair::generate("test");
    let verifier = Arc::new(TokenVerifier::new(&keys.public_key(), Audience::Tenant));
    let state = AppState::new(db.app.clone(), verifier, SERVICE_TOKEN);
    App {
        db,
        keys,
        router: api::router(state),
    }
}

impl App {
    pub fn token(&self, tenant: TenantId, roles: &[Role]) -> String {
        let claims = Claims::new(
            Uuid::new_v4(),
            Some(tenant),
            roles.to_vec(),
            Audience::Tenant,
            600,
        );
        TokenSigner::new(&self.keys).sign(&claims).unwrap()
    }

    /// Токен audience tenant, подписанный верно, но без тенанта.
    pub fn tenantless_token(&self) -> String {
        let claims = Claims::new(
            Uuid::new_v4(),
            None,
            vec![Role::Owner],
            Audience::Tenant,
            600,
        );
        TokenSigner::new(&self.keys).sign(&claims).unwrap()
    }

    /// Токен платформенного администратора (audience platform) тем же ключом.
    pub fn platform_token(&self) -> String {
        let claims = Claims::new(
            Uuid::new_v4(),
            None,
            vec![Role::PlatformAdmin],
            Audience::Platform,
            600,
        );
        TokenSigner::new(&self.keys).sign(&claims).unwrap()
    }

    pub async fn send(
        &self,
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (u16, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let res = self
            .router
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = res.status().as_u16();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    pub async fn close(self) {
        self.db.close().await;
    }
}
