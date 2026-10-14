#![allow(dead_code)]
use admin::{
    MIGRATOR,
    api::{self, AppState},
    infrastructure::admins,
};
use axum::{Router, body::Body, http::Request};
use http_body_util::BodyExt;
use platform_auth::{Audience, Claims, KeyPair, Role, TokenSigner};
use platform_db::testing::TestDb;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

pub const SERVICE_TOKEN: &str = "test-service-token";
pub const ADMIN_EMAIL: &str = "root@platform.io";
pub const ADMIN_PASSWORD: &str = "root-password-1";

pub struct Ctx {
    pub db: TestDb,
    pub keys: KeyPair,
    router: Router,
}

pub async fn setup() -> Ctx {
    let db = TestDb::create(&MIGRATOR).await;
    admins::seed(&db.app, ADMIN_EMAIL, ADMIN_PASSWORD)
        .await
        .unwrap();
    let keys = KeyPair::generate("admin-test");
    let state = AppState::new(db.app.clone(), &keys, SERVICE_TOKEN);
    Ctx {
        db,
        keys,
        router: api::router(state),
    }
}

impl Ctx {
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
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    pub async fn post(&self, uri: &str, body: Value) -> (u16, Value) {
        self.send("POST", uri, None, body).await
    }

    /// Токен платформенного администратора через настоящий вход.
    pub async fn admin_token(&self) -> String {
        let (s, body) = self
            .post(
                "/auth/login",
                serde_json::json!({"email": ADMIN_EMAIL, "password": ADMIN_PASSWORD}),
            )
            .await;
        assert_eq!(s, 200, "{body}");
        body["access_token"].as_str().unwrap().to_string()
    }

    /// Токен с произвольной audience, подписанный ключом сервиса (для негативных тестов).
    pub fn forge(&self, aud: Audience, roles: Vec<Role>) -> String {
        let claims = Claims::new(Uuid::new_v4(), None, roles, aud, 600);
        TokenSigner::new(&self.keys).sign(&claims).unwrap()
    }

    pub async fn close(self) {
        self.db.close().await;
    }
}
