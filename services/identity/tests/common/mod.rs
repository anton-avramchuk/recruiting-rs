#![allow(dead_code)]
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{Router, body::Body, http::Request};
use http_body_util::BodyExt;
use identity::{
    MIGRATOR,
    api::{self, AppState},
    application::tenants::{DirectoryError, TenantDirectory, TenantInfo, TenantStatus},
    infrastructure::password,
};
use platform_auth::{Audience, KeyPair, TokenVerifier};
use platform_core::tenant::TenantId;
use platform_db::testing::TestDb;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

#[derive(Default, Clone)]
pub struct FakeDirectory(pub Arc<Mutex<HashMap<String, TenantInfo>>>);

impl FakeDirectory {
    pub fn add(&self, slug: &str, status: TenantStatus) -> TenantId {
        let id = TenantId::new();
        self.0
            .lock()
            .unwrap()
            .insert(slug.into(), TenantInfo { id, status });
        id
    }
    pub fn set_status(&self, slug: &str, status: TenantStatus) {
        self.0.lock().unwrap().get_mut(slug).unwrap().status = status;
    }
}

#[async_trait::async_trait]
impl TenantDirectory for FakeDirectory {
    async fn resolve(&self, slug: &str) -> Result<Option<TenantInfo>, DirectoryError> {
        Ok(self.0.lock().unwrap().get(slug).cloned())
    }

    async fn status(&self, id: TenantId) -> Result<Option<TenantStatus>, DirectoryError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .values()
            .find(|t| t.id == id)
            .map(|t| t.status))
    }
}

pub struct Ctx {
    pub db: TestDb,
    pub dir: FakeDirectory,
    pub verifier: TokenVerifier,
    router: Router,
}

pub async fn setup() -> Ctx {
    let db = TestDb::create(&MIGRATOR).await;
    let dir = FakeDirectory::default();
    let keys = KeyPair::generate("test");
    let verifier = TokenVerifier::new(&keys.public_key(), Audience::Tenant);
    let state = AppState::new(db.app.clone(), Arc::new(dir.clone()), &keys);
    Ctx {
        db,
        dir,
        verifier,
        router: api::router(state),
    }
}

impl Ctx {
    pub async fn seed_user(
        &self,
        tenant: TenantId,
        email: &str,
        password: &str,
        roles: &[&str],
    ) -> Uuid {
        let hash = password::hash(password).unwrap();
        let roles: Vec<String> = roles.iter().map(|r| r.to_string()).collect();
        sqlx::query_scalar(
            "INSERT INTO users (tenant_id, email, password_hash, roles) VALUES ($1,$2,$3,$4) RETURNING id",
        )
        .bind(tenant.0)
        .bind(email)
        .bind(hash)
        .bind(roles)
        .fetch_one(&self.db.owner)
        .await
        .unwrap()
    }

    pub async fn send(
        &self,
        method: &str,
        uri: &str,
        bearer: Option<&str>,
        body: Value,
    ) -> (u16, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(t) = bearer {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let res = self
            .router
            .clone()
            .oneshot(req.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = res.status().as_u16();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json)
    }

    pub async fn post(&self, uri: &str, body: Value) -> (u16, Value) {
        self.send("POST", uri, None, body).await
    }

    pub async fn close(self) {
        self.db.close().await;
    }
}
