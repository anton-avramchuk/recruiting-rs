use std::sync::{Arc, atomic::Ordering};

use bff_core::{
    MemorySessionStore,
    testing::{Client, Upstream},
};
use bff_crm::{CrmConfig, build};
use serde_json::json;

struct World {
    client_service: Upstream,
    candidate: Upstream,
    recruitment: Upstream,
    client: Client,
    csrf: String,
}

async fn world(client_url: Option<String>) -> World {
    let identity = Upstream::new();
    let shared = |u: &Upstream| Upstream {
        valid_access: u.valid_access.clone(),
        ..Upstream::new()
    };
    let candidate = shared(&identity);
    let recruitment = shared(&identity);
    let client_service = shared(&identity);
    let cfg = CrmConfig {
        identity_url: identity.serve().await,
        candidate_url: candidate.serve().await,
        recruitment_url: recruitment.serve().await,
        comment_url: "http://127.0.0.1:9".into(),
        client_url: match client_url {
            Some(url) => url,
            None => client_service.serve().await,
        },
        cors_origins: Vec::new(),
        cookie_secure: true,
        base_domain: None,
        session_ttl_secs: 3600,
    };
    let client = Client::new(
        build(&cfg, Arc::new(MemorySessionStore::new())),
        "crm_session",
    );
    let login = client
        .send(
            "POST",
            "/bff/login",
            &[("x-csrf-token", "x")],
            Some(json!({"tenant": "acme", "email": "a@b.io", "password": "good-password"})),
        )
        .await;
    assert_eq!(login.status, 200);
    let csrf = login.body["csrf_token"].as_str().unwrap().to_string();
    World {
        client_service,
        candidate,
        recruitment,
        client,
        csrf,
    }
}

#[tokio::test]
async fn client_routes_reach_only_the_client_service() {
    let w = world(None).await;
    for path in [
        "/api/clients",
        "/api/clients/1",
        "/api/contacts/1",
        "/api/positions",
        "/api/positions/1",
    ] {
        let res = w.client.send("GET", path, &[], None).await;
        assert_eq!(res.status, 200, "{path}");
    }
    assert_eq!(w.client_service.api_calls.load(Ordering::SeqCst), 5);
    assert_eq!(w.candidate.api_calls.load(Ordering::SeqCst), 0);
    assert_eq!(w.recruitment.api_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn client_writes_need_the_csrf_token() {
    let w = world(None).await;
    for (method, path) in [
        ("POST", "/api/clients"),
        ("PATCH", "/api/clients/1"),
        ("POST", "/api/clients/1/contacts"),
        ("PATCH", "/api/contacts/1"),
        ("DELETE", "/api/contacts/1"),
        ("POST", "/api/positions"),
        ("DELETE", "/api/positions/1"),
    ] {
        let res = w.client.send(method, path, &[], Some(json!({}))).await;
        assert_eq!(res.status, 403, "{method} {path}");
    }
    assert_eq!(w.client_service.api_calls.load(Ordering::SeqCst), 0);
    let ok = w
        .client
        .send(
            "POST",
            "/api/clients",
            &[("x-csrf-token", &w.csrf)],
            Some(json!({"name": "Acme"})),
        )
        .await;
    assert_eq!(ok.status, 200);
}

#[tokio::test]
async fn internal_paths_and_lookalikes_stay_closed() {
    let w = world(None).await;
    for path in [
        "/api/internal/tenants/1/client-changes",
        "/api/clients-export",
        "/api/clientsx",
        "/api/contacts-all",
        "/api/positionsx",
    ] {
        for method in ["GET", "POST"] {
            let res = w
                .client
                .send(method, path, &[("x-csrf-token", &w.csrf)], None)
                .await;
            assert_eq!(res.status, 404, "{method} {path}");
        }
    }
    assert_eq!(w.client_service.api_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_dead_client_service_does_not_break_the_rest() {
    let w = world(Some("http://127.0.0.1:9".into())).await;
    let res = w.client.send("GET", "/api/clients", &[], None).await;
    assert_eq!(res.status, 502);
    for path in ["/api/users", "/api/persons", "/api/vacancies"] {
        assert_eq!(
            w.client.send("GET", path, &[], None).await.status,
            200,
            "{path}"
        );
    }
}
