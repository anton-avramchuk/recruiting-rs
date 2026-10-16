//! Слои независимы: остановка bff-admin не влияет на работу bff-crm.
use std::sync::Arc;

use bff_admin::AdminConfig;
use bff_core::{MemorySessionStore, testing::Upstream};
use bff_crm::CrmConfig;
use serde_json::json;
use tokio::task::JoinHandle;

async fn serve(app: axum::Router) -> (String, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), handle)
}

#[tokio::test]
async fn stopping_the_admin_bff_does_not_affect_the_crm_bff() {
    let backend = Upstream::new().serve().await;
    let (crm_url, _crm) = serve(bff_crm::build(
        &CrmConfig {
            identity_url: backend.clone(),
            candidate_url: backend.clone(),
            recruitment_url: "http://127.0.0.1:9".into(),
            client_url: "http://127.0.0.1:9".into(),
            comment_url: "http://127.0.0.1:9".into(),
            cors_origins: Vec::new(),
            cookie_secure: false,
            base_domain: None,
            session_ttl_secs: 3600,
        },
        Arc::new(MemorySessionStore::new()),
    ))
    .await;
    let (admin_url, admin) = serve(bff_admin::build(
        &AdminConfig {
            cors_origins: Vec::new(),
            admin_url: backend,
            cookie_secure: false,
            session_ttl_secs: 3600,
        },
        Arc::new(MemorySessionStore::new()),
    ))
    .await;

    let http = reqwest::Client::new();
    let health = |url: &str| {
        let http = http.clone();
        let url = format!("{url}/health");
        async move { http.get(url).send().await.map(|r| r.status().as_u16()) }
    };
    assert_eq!(health(&crm_url).await.unwrap(), 200);
    assert_eq!(health(&admin_url).await.unwrap(), 200);

    // останавливаем админский слой
    admin.abort();
    let _ = admin.await;
    assert!(health(&admin_url).await.is_err(), "admin BFF is down");

    // клиентский слой продолжает работать целиком: вход, сессия, прокси
    assert_eq!(health(&crm_url).await.unwrap(), 200);
    let login = http
        .post(format!("{crm_url}/bff/login"))
        .header("x-csrf-token", "x")
        .json(&json!({"tenant": "acme", "email": "a@b.io", "password": "good-password"}))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);
    let cookie = login.headers()["set-cookie"].to_str().unwrap();
    let cookie = cookie.split(';').next().unwrap().to_string();
    let users = http
        .get(format!("{crm_url}/api/users"))
        .header("cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(users.status(), 200);
}
