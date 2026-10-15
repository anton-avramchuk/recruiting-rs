#![allow(dead_code)]
use std::sync::Arc;

use async_trait::async_trait;
use axum::{Router, body::Body, http::Request};
use candidate_contracts::PersonChangesPage;
use http_body_util::BodyExt;
use platform_auth::{Audience, Claims, KeyPair, Role, TokenSigner, TokenVerifier};
use platform_core::tenant::TenantId;
use platform_db::testing::TestDb;
use recruitment::{
    MIGRATOR,
    api::{self, AppState},
    application::{FeedError, PersonFeed},
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

/// Лента без изменений: в API-тестах люди кладутся в проекцию напрямую.
pub struct EmptyFeed;

#[async_trait]
impl PersonFeed for EmptyFeed {
    async fn changes(
        &self,
        _tenant: TenantId,
        after: i64,
        _limit: i64,
    ) -> Result<PersonChangesPage, FeedError> {
        Ok(PersonChangesPage {
            items: vec![],
            next: after,
        })
    }
}

pub struct App {
    pub db: TestDb,
    keys: KeyPair,
    router: Router,
}

pub async fn setup() -> App {
    setup_with_feed(Arc::new(EmptyFeed)).await
}

pub async fn setup_with_feed(feed: Arc<dyn PersonFeed>) -> App {
    let db = TestDb::create(&MIGRATOR).await;
    let keys = KeyPair::generate("test");
    let verifier = Arc::new(TokenVerifier::new(&keys.public_key(), Audience::Tenant));
    let state = AppState::new(db.app.clone(), verifier, feed);
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

    pub async fn send(
        &self,
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (u16, Value) {
        let (status, _, value) = self.send_full(method, uri, token, body).await;
        (status, value)
    }

    pub async fn send_full(
        &self,
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (u16, axum::http::HeaderMap, Value) {
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
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            headers,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// Кладёт человека в проекцию (мимо RLS, от владельца).
    pub async fn add_person(&self, tenant: TenantId, name: &str, skills: &[&str]) -> Uuid {
        self.add_person_full(tenant, name, skills, json!({})).await
    }

    /// Поля `source`, `city`, `expected_salary`, `email` задаются через `extra`.
    pub async fn add_person_full(
        &self,
        tenant: TenantId,
        name: &str,
        skills: &[&str],
        extra: Value,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let skills: Vec<String> = skills.iter().map(|s| s.to_string()).collect();
        sqlx::query(
            "INSERT INTO people (tenant_id, person_id, full_name, source, city, expected_salary, email, skills)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(tenant.0)
        .bind(id)
        .bind(name)
        .bind(extra["source"].as_str())
        .bind(extra["city"].as_str())
        .bind(extra["expected_salary"].as_i64())
        .bind(extra["email"].as_str())
        .bind(skills)
        .execute(&self.db.owner)
        .await
        .unwrap();
        id
    }

    pub async fn add_vacancy(&self, token: &str, body: Value) -> Value {
        let (s, v) = self
            .send("POST", "/vacancies", Some(token), Some(body))
            .await;
        assert_eq!(s, 201, "{v}");
        v
    }

    pub async fn add_application(&self, token: &str, person: Uuid, vacancy: &Value) -> Value {
        let (s, v) = self
            .send(
                "POST",
                "/applications",
                Some(token),
                Some(json!({"person_id": person, "vacancy_id": vacancy["id"]})),
            )
            .await;
        assert_eq!(s, 201, "{v}");
        v
    }

    /// Сдвигает момент входа на этап в прошлое.
    pub async fn age_stage(&self, application_id: &Value, hours: i64) {
        let id: Uuid = application_id.as_str().unwrap().parse().unwrap();
        sqlx::query(
            "UPDATE applications SET stage_changed_at = now() - make_interval(hours => $2::int) WHERE id = $1",
        )
        .bind(id)
        .bind(hours as i32)
        .execute(&self.db.owner)
        .await
        .unwrap();
    }

    pub async fn close(self) {
        self.db.close().await;
    }
}
