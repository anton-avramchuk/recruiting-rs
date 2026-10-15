//! Синхронизация проекции заказчиков с настоящим сервисом client (в процессе теста).
mod common;

use std::sync::Arc;

use client::api::AppState as ClientState;
use common::{EmptyFeed, setup_with_feeds};
use platform_auth::{Audience, Claims, KeyPair, Role, TokenSigner, TokenVerifier};
use platform_core::tenant::TenantId;
use platform_db::testing::TestDb;
use recruitment::application::HttpClientFeed;
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use uuid::Uuid;

const SERVICE_TOKEN: &str = "client-sync-token";

struct ClientService {
    db: TestDb,
    url: String,
    keys: KeyPair,
    server: Option<JoinHandle<()>>,
    http: reqwest::Client,
}

impl ClientService {
    async fn start() -> Self {
        let db = TestDb::create(&client::MIGRATOR).await;
        let keys = KeyPair::generate("client-test");
        let verifier = Arc::new(TokenVerifier::new(&keys.public_key(), Audience::Tenant));
        let router = client::api::router(ClientState::new(db.app.clone(), verifier, SERVICE_TOKEN));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            db,
            url,
            keys,
            server: Some(server),
            http: reqwest::Client::new(),
        }
    }

    fn token(&self, tenant: TenantId) -> String {
        let claims = Claims::new(
            Uuid::new_v4(),
            Some(tenant),
            vec![Role::Owner],
            Audience::Tenant,
            600,
        );
        TokenSigner::new(&self.keys).sign(&claims).unwrap()
    }

    async fn call(
        &self,
        tenant: TenantId,
        method: reqwest::Method,
        path: &str,
        body: Value,
    ) -> Value {
        let res = self
            .http
            .request(method, format!("{}{path}", self.url))
            .bearer_auth(self.token(tenant))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert!(res.status().is_success(), "{path}: {}", res.status());
        res.json().await.unwrap_or(Value::Null)
    }

    async fn close(mut self) {
        if let Some(s) = self.server.take() {
            s.abort();
        }
        self.db.close().await;
    }
}

#[tokio::test]
async fn names_and_positions_flow_from_the_client_service_into_vacancies() {
    let clients = ClientService::start().await;
    let feed = Arc::new(HttpClientFeed::new(&clients.url, SERVICE_TOKEN));
    let app = setup_with_feeds(Arc::new(EmptyFeed), feed).await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let post = reqwest::Method::POST;
    let patch = reqwest::Method::PATCH;

    let position = clients
        .call(
            tenant,
            post.clone(),
            "/positions",
            json!({"name": "Директор по персоналу"}),
        )
        .await;
    let client = clients
        .call(tenant, post.clone(), "/clients", json!({"name": "Acme"}))
        .await;
    let client_id = client["id"].as_str().unwrap();
    let contact = clients
        .call(
            tenant,
            post.clone(),
            &format!("/clients/{client_id}/contacts"),
            json!({"full_name": "Мария Петрова", "position_id": position["id"]}),
        )
        .await;

    let (s, v) = app
        .send(
            "POST",
            "/vacancies",
            Some(&token),
            Some(json!({"title": "Go", "client_id": client["id"], "contact_id": contact["id"]})),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    assert_eq!(v["client"]["name"], "Acme");
    assert_eq!(v["contact"]["full_name"], "Мария Петрова");
    assert_eq!(v["contact"]["position"], "Директор по персоналу");

    // переименования доезжают при следующем чтении
    clients
        .call(
            tenant,
            patch.clone(),
            &format!("/positions/{}", position["id"].as_str().unwrap()),
            json!({"name": "HR-директор"}),
        )
        .await;
    clients
        .call(
            tenant,
            patch.clone(),
            &format!("/clients/{client_id}"),
            json!({"name": "Acme Corp"}),
        )
        .await;
    let uri = format!("/vacancies/{}", v["id"].as_str().unwrap());
    let (_, card) = app.send("GET", &uri, Some(&token), None).await;
    assert_eq!(card["client"]["name"], "Acme Corp");
    assert_eq!(card["contact"]["position"], "HR-директор");

    // архивация видна, удаление контакта тоже
    clients
        .call(
            tenant,
            patch,
            &format!("/clients/{client_id}"),
            json!({"archived": true}),
        )
        .await;
    clients
        .call(
            tenant,
            reqwest::Method::DELETE,
            &format!("/contacts/{}", contact["id"].as_str().unwrap()),
            json!({}),
        )
        .await;
    let (_, card) = app.send("GET", &uri, Some(&token), None).await;
    assert_eq!(card["client"]["archived"], true);
    assert_eq!(card["contact"], json!(null));

    // новая вакансия для архивного заказчика уже невозможна
    let (s, p) = app
        .send(
            "POST",
            "/vacancies",
            Some(&token),
            Some(json!({"title": "Rust", "client_id": client["id"]})),
        )
        .await;
    assert_eq!((s, p["field"].as_str()), (400, Some("client_id")), "{p}");

    app.close().await;
    clients.close().await;
}
