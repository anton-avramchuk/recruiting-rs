//! Сквозной сценарий платформы по HTTP: все четыре сервиса и два BFF - настоящие процессы.
use std::time::Duration;

use e2e::{ADMIN_EMAIL, ADMIN_PASSWORD, Stack};
use reqwest::{Client, Response};
use serde_json::{Value, json};

/// Клиент одного слоя: хранит cookie сессии и CSRF-токен.
struct Session {
    http: Client,
    base: String,
    csrf: String,
}

impl Session {
    async fn login(base: &str, body: Value) -> (u16, Self) {
        let http = Client::builder().cookie_store(true).build().unwrap();
        let res = http
            .post(format!("{base}/bff/login"))
            .header("x-csrf-token", "login")
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = res.status().as_u16();
        let csrf = res
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| v["csrf_token"].as_str().map(str::to_string))
            .unwrap_or_default();
        (
            status,
            Self {
                http,
                base: base.into(),
                csrf,
            },
        )
    }

    async fn get(&self, path: &str) -> Response {
        self.http
            .get(format!("{}{path}", self.base))
            .send()
            .await
            .unwrap()
    }

    async fn post(&self, path: &str, body: Value) -> Response {
        self.http
            .post(format!("{}{path}", self.base))
            .header("x-csrf-token", &self.csrf)
            .json(&body)
            .send()
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn tenant_lifecycle_across_the_whole_platform() {
    let stack = Stack::start().await;

    // 1. администратор платформы входит через админский BFF
    let (status, admin) = Session::login(
        &stack.bff_admin_url,
        json!({"email": ADMIN_EMAIL, "password": ADMIN_PASSWORD}),
    )
    .await;
    assert_eq!(status, 200);

    // 2. создаёт тенанта вместе с владельцем
    let res = admin
        .post(
            "/api/tenants",
            json!({
                "slug": "acme", "name": "Acme Inc",
                "owner": {"email": "boss@acme.io", "password": "boss-password-1", "display_name": "Boss"}
            }),
        )
        .await;
    assert_eq!(res.status(), 201, "{}", res.text().await.unwrap());
    let tenant: Value = admin.get("/api/tenants?q=acme").await.json().await.unwrap();
    assert_eq!(tenant["total"], 1);
    let tenant_id = tenant["items"][0]["id"].as_str().unwrap().to_string();

    // 3. владелец входит через клиентский BFF и заводит рекрутера
    let (status, owner) = Session::login(
        &stack.bff_crm_url,
        json!({"tenant": "acme", "email": "boss@acme.io", "password": "boss-password-1"}),
    )
    .await;
    assert_eq!(status, 200);
    let res = owner
        .post(
            "/api/users",
            json!({"email": "rec@acme.io", "password": "recruiter-pass-1", "roles": ["recruiter"]}),
        )
        .await;
    assert_eq!(res.status(), 201);
    let users: Value = owner.get("/api/users").await.json().await.unwrap();
    assert_eq!(users.as_array().unwrap().len(), 2);

    // 4. рекрутер входит, но управлять пользователями не может
    let recruiter_login =
        json!({"tenant": "acme", "email": "rec@acme.io", "password": "recruiter-pass-1"});
    let (status, recruiter) = Session::login(&stack.bff_crm_url, recruiter_login.clone()).await;
    assert_eq!(status, 200);
    let res = recruiter
        .post(
            "/api/users",
            json!({"email": "x@acme.io", "password": "whatever-123", "roles": ["viewer"]}),
        )
        .await;
    assert_eq!(res.status(), 403);

    // 5. админский слой недоступен клиентскому BFF и наоборот
    assert_eq!(owner.get("/api/tenants").await.status(), 404);
    assert_eq!(admin.get("/api/users").await.status(), 404);

    // 6. администратор приостанавливает тенанта
    let res = admin
        .post(&format!("/api/tenants/{tenant_id}/suspend"), json!(null))
        .await;
    assert_eq!(res.status(), 200);
    tokio::time::sleep(Duration::from_millis(1500)).await; // TTL кэша статуса тенанта в identity

    // новые входы блокируются
    let (status, _) = Session::login(&stack.bff_crm_url, recruiter_login.clone()).await;
    assert_eq!(status, 403);
    // действующая сессия не может обновить токен и завершается
    assert_eq!(owner.get("/api/users").await.status(), 401);
    let state: Value = owner.get("/bff/session").await.json().await.unwrap();
    assert_eq!(state["authenticated"], false);

    // 7. активация возвращает доступ; данные тенанта сохранились
    let res = admin
        .post(&format!("/api/tenants/{tenant_id}/activate"), json!(null))
        .await;
    assert_eq!(res.status(), 200);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let (status, owner_again) = Session::login(
        &stack.bff_crm_url,
        json!({"tenant": "acme", "email": "boss@acme.io", "password": "boss-password-1"}),
    )
    .await;
    assert_eq!(status, 200);
    let users: Value = owner_again.get("/api/users").await.json().await.unwrap();
    assert_eq!(
        users.as_array().unwrap().len(),
        2,
        "data survives suspension"
    );

    stack.stop().await;
}
