use std::sync::{Arc, atomic::Ordering};

use bff_core::{
    MemorySessionStore,
    testing::{Client, Upstream},
};
use bff_crm::{CrmConfig, build};
use serde_json::json;

struct World {
    identity: Upstream,
    candidate: Upstream,
    client: Client,
    csrf: String,
}

async fn world() -> World {
    let identity = Upstream::new();
    let candidate = Upstream {
        valid_access: identity.valid_access.clone(),
        ..Upstream::new()
    };
    let cfg = CrmConfig {
        identity_url: identity.serve().await,
        candidate_url: candidate.serve().await,
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
        identity,
        candidate,
        client,
        csrf,
    }
}

#[tokio::test]
async fn users_and_candidates_reach_their_own_services() {
    let w = world().await;
    assert_eq!(
        w.client.send("GET", "/api/users", &[], None).await.status,
        200
    );
    assert_eq!(
        w.client.send("GET", "/api/persons", &[], None).await.status,
        200
    );
    assert_eq!(w.identity.api_calls.load(Ordering::SeqCst), 1);
    assert_eq!(w.candidate.api_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn candidate_writes_need_the_csrf_token() {
    let w = world().await;
    let body = || Some(json!({"full_name": "Иван"}));
    for method in ["POST", "PATCH", "PUT", "DELETE"] {
        let res = w.client.send(method, "/api/persons/1", &[], body()).await;
        assert_eq!(res.status, 403, "{method}");
    }
    assert_eq!(w.candidate.api_calls.load(Ordering::SeqCst), 0);
    let ok = w
        .client
        .send("POST", "/api/persons", &[("x-csrf-token", &w.csrf)], body())
        .await;
    assert_eq!(ok.status, 200);
}

#[tokio::test]
async fn admin_internal_and_service_paths_stay_closed() {
    let w = world().await;
    for path in [
        "/api/tenants",
        "/api/internal/persons",
        "/api/persons-export",
        "/api/health",
        "/api/openapi.json",
    ] {
        for method in ["GET", "POST"] {
            let res = w
                .client
                .send(method, path, &[("x-csrf-token", &w.csrf)], None)
                .await;
            assert_eq!(res.status, 404, "{method} {path}");
        }
    }
    assert_eq!(w.candidate.api_calls.load(Ordering::SeqCst), 0);
    assert_eq!(w.identity.api_calls.load(Ordering::SeqCst), 0);
}
