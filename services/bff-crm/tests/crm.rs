use std::{sync::Arc, sync::atomic::Ordering};

use bff_core::{
    MemorySessionStore,
    testing::{Client, Upstream},
};
use bff_crm::{CrmConfig, build};
use serde_json::{Value, json};

struct World {
    upstream: Upstream,
    client: Client,
}

async fn world(base_domain: Option<&str>) -> World {
    let upstream = Upstream::new();
    let cfg = CrmConfig {
        identity_url: upstream.serve().await,
        candidate_url: "http://127.0.0.1:9".into(),
        recruitment_url: "http://127.0.0.1:9".into(),
        cookie_secure: true,
        base_domain: base_domain.map(Into::into),
        session_ttl_secs: 3600,
    };
    let router = build(&cfg, Arc::new(MemorySessionStore::new()));
    World {
        upstream,
        client: Client::new(router, "crm_session"),
    }
}

fn creds(tenant: Option<&str>) -> Value {
    let mut v = json!({"email": "a@b.io", "password": "good-password", "junk": "dropped"});
    if let Some(t) = tenant {
        v["tenant"] = json!(t);
    }
    v
}

#[tokio::test]
async fn login_forwards_the_tenant_from_the_form() {
    let w = world(None).await;
    let res = w
        .client
        .send(
            "POST",
            "/bff/login",
            &[("x-csrf-token", "x")],
            Some(creds(Some("acme"))),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.raw);
    let sent = w.upstream.last_login_body.lock().unwrap().clone().unwrap();
    assert_eq!(
        sent,
        json!({"tenant": "acme", "email": "a@b.io", "password": "good-password"})
    );
}

#[tokio::test]
async fn subdomain_wins_over_the_form_value() {
    let w = world(Some("crm.test")).await;
    let res = w
        .client
        .send(
            "POST",
            "/bff/login",
            &[("x-csrf-token", "x"), ("host", "acme.crm.test")],
            Some(creds(Some("someone-else"))),
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.raw);
    let sent = w.upstream.last_login_body.lock().unwrap().clone().unwrap();
    assert_eq!(sent["tenant"], "acme");
}

#[tokio::test]
async fn login_without_any_tenant_is_a_bad_request() {
    let w = world(Some("crm.test")).await;
    for host in ["crm.test", "evil.example.com"] {
        let res = w
            .client
            .send(
                "POST",
                "/bff/login",
                &[("x-csrf-token", "x"), ("host", host)],
                Some(creds(None)),
            )
            .await;
        assert_eq!(res.status, 400, "{host}");
    }
    assert!(w.upstream.last_login_body.lock().unwrap().is_none());
}

#[tokio::test]
async fn only_tenant_user_routes_are_exposed() {
    let w = world(None).await;
    w.client
        .send(
            "POST",
            "/bff/login",
            &[("x-csrf-token", "x")],
            Some(creds(Some("acme"))),
        )
        .await;

    // пользовательский маршрут проксируется
    assert_eq!(
        w.client.send("GET", "/api/users", &[], None).await.status,
        200
    );
    let calls_after_allowed = w.upstream.api_calls.load(Ordering::SeqCst);

    // admin-маршруты, internal и auth недоступны через клиентский BFF
    for path in [
        "/api/tenants",
        "/api/tenants/123/suspend",
        "/api/admin/secret",
        "/api/internal/tenants/by-slug/acme",
        "/api/auth/login",
        "/api/me",
    ] {
        for method in ["GET", "POST"] {
            let res = w
                .client
                .send(method, path, &[("x-csrf-token", "x")], None)
                .await;
            assert_eq!(res.status, 404, "{method} {path}");
        }
    }
    assert_eq!(
        w.upstream.api_calls.load(Ordering::SeqCst),
        calls_after_allowed
    );
}

#[tokio::test]
async fn serves_health_and_openapi() {
    let w = world(None).await;
    assert_eq!(w.client.send("GET", "/health", &[], None).await.status, 200);

    let doc = w.client.send("GET", "/openapi.json", &[], None).await;
    assert_eq!(doc.status, 200);
    for (path, method) in [
        ("/bff/login", "post"),
        ("/bff/logout", "post"),
        ("/bff/session", "get"),
    ] {
        assert!(
            doc.body["paths"][path][method].is_object(),
            "{method} {path}"
        );
    }
}
