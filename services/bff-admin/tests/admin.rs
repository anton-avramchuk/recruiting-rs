use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use bff_admin::{AdminConfig, build};
use bff_core::{
    MemorySessionStore,
    testing::{Client, Upstream},
};
use serde_json::{Value, json};

struct World {
    upstream: Upstream,
    client: Client,
}

async fn world() -> World {
    let upstream = Upstream::new();
    let cfg = AdminConfig {
        admin_url: upstream.serve().await,
        cors_origins: Vec::new(),
        cookie_secure: true,
        session_ttl_secs: 3600,
    };
    World {
        client: Client::new(
            build(&cfg, Arc::new(MemorySessionStore::new())),
            "admin_session",
        ),
        upstream,
    }
}

fn creds() -> Value {
    json!({"email": "root@platform.io", "password": "good-password", "tenant": "ignored"})
}

async fn login(w: &World) {
    let res = w
        .client
        .send(
            "POST",
            "/bff/login",
            &[("x-csrf-token", "x")],
            Some(creds()),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.raw);
}

#[tokio::test]
async fn login_sends_only_email_and_password() {
    let w = world().await;
    login(&w).await;
    assert_eq!(
        w.upstream.last_login_body.lock().unwrap().clone().unwrap(),
        json!({"email": "root@platform.io", "password": "good-password"})
    );
}

#[tokio::test]
async fn uses_its_own_cookie_name() {
    let w = world().await;
    let res = w
        .client
        .send(
            "POST",
            "/bff/login",
            &[("x-csrf-token", "x")],
            Some(creds()),
        )
        .await;
    let sc = res.headers.get("set-cookie").unwrap().to_str().unwrap();
    assert!(sc.starts_with("admin_session="), "{sc}");
    assert!(sc.contains("HttpOnly") && sc.contains("Secure") && sc.contains("SameSite=Lax"));
}

#[tokio::test]
async fn a_client_layer_cookie_is_not_a_session() {
    let w = world().await;
    login(&w).await;
    let id = w.client.session_cookie_value().unwrap();
    *w.client.cookie.lock().unwrap() = Some(format!("crm_session={id}"));
    let res = w.client.send("GET", "/bff/session", &[], None).await;
    assert_eq!(res.body["authenticated"], false);
}

#[tokio::test]
async fn exposes_tenant_management_but_not_tenant_user_routes() {
    let w = world().await;
    login(&w).await;
    assert_eq!(
        w.client.send("GET", "/api/tenants", &[], None).await.status,
        200
    );
    assert_eq!(w.client.send("GET", "/api/me", &[], None).await.status, 200);
    let allowed_calls = w.upstream.api_calls.load(Ordering::SeqCst);

    for path in [
        "/api/users",
        "/api/admin/secret",
        "/api/internal/tenants/by-slug/acme",
        "/api/auth/login",
    ] {
        for method in ["GET", "POST"] {
            let res = w
                .client
                .send(method, path, &[("x-csrf-token", "x")], None)
                .await;
            assert_eq!(res.status, 404, "{method} {path}");
        }
    }
    assert_eq!(w.upstream.api_calls.load(Ordering::SeqCst), allowed_calls);
}

#[tokio::test]
async fn expired_admin_token_ends_the_session_without_refresh() {
    let w = world().await;
    *w.upstream.access_ttl_secs.lock().unwrap() = 1;
    login(&w).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let res = w.client.send("GET", "/api/tenants", &[], None).await;
    assert_eq!(res.status, 401);
    assert_eq!(w.upstream.refresh_calls.load(Ordering::SeqCst), 0);
    let after = w.client.send("GET", "/bff/session", &[], None).await;
    assert_eq!(after.body["authenticated"], false);
}

#[tokio::test]
async fn serves_health_and_openapi() {
    let w = world().await;
    assert_eq!(w.client.send("GET", "/health", &[], None).await.status, 200);
    let doc = w.client.send("GET", "/openapi.json", &[], None).await;
    assert_eq!(doc.status, 200);
    assert!(doc.body["paths"]["/bff/login"]["post"].is_object());
    assert_eq!(doc.body["info"]["title"], "Admin BFF");
}

#[tokio::test]
async fn cors_is_enabled_by_configuration() {
    let upstream = Upstream::new();
    let cfg = AdminConfig {
        admin_url: upstream.serve().await,
        cors_origins: vec!["http://localhost:3010".into()],
        cookie_secure: true,
        session_ttl_secs: 3600,
    };
    let client = Client::new(
        build(&cfg, Arc::new(MemorySessionStore::new())),
        "admin_session",
    );
    let res = client
        .send(
            "GET",
            "/bff/session",
            &[("origin", "http://localhost:3010")],
            None,
        )
        .await;
    assert_eq!(
        res.headers["access-control-allow-origin"],
        "http://localhost:3010"
    );
    let preflight = client
        .send(
            "OPTIONS",
            "/api/tenants",
            &[
                ("origin", "http://localhost:3010"),
                ("access-control-request-method", "POST"),
            ],
            None,
        )
        .await;
    assert_eq!(
        preflight.headers["access-control-allow-credentials"],
        "true"
    );
}
