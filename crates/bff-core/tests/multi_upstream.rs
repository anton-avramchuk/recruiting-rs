//! Один BFF, несколько бэкенд-сервисов: маршруты уходят по allowlist, токен сессии общий.
mod common;

use std::{
    collections::HashMap,
    sync::{Arc, atomic::Ordering},
};

use bff_core::{
    AllowRule, Bff, HttpUpstreamAuth, MemorySessionStore, router,
    testing::{Client, Upstream},
};
use common::{COOKIE, config};

struct World {
    identity: Upstream,
    candidate: Upstream,
    client: Client,
}

async fn world_with(candidate_url: Option<String>) -> World {
    let identity = Upstream::new();
    // сервис кандидатов принимает те же токены, что выдал identity
    let candidate = Upstream {
        valid_access: identity.valid_access.clone(),
        ..Upstream::new()
    };
    let identity_url = identity.serve().await;
    let candidate_url = match candidate_url {
        Some(url) => url,
        None => candidate.serve().await,
    };

    let mut cfg = config(&identity_url);
    cfg.allow = vec![
        AllowRule::any("/users"),
        AllowRule::any("/persons").to("candidate"),
    ];
    cfg.upstreams = HashMap::from([("candidate".to_string(), candidate_url)]);
    let auth = Arc::new(HttpUpstreamAuth::new(
        &identity_url,
        "/auth/login",
        Some("/auth/refresh"),
        Some("/auth/logout"),
    ));
    let bff = Bff::new(cfg, Arc::new(MemorySessionStore::new()), auth);
    World {
        identity,
        candidate,
        client: Client::new(router(bff), COOKIE),
    }
}

async fn world() -> World {
    world_with(None).await
}

#[tokio::test]
async fn routes_go_to_their_services_with_the_same_session_token() {
    let w = world().await;
    assert_eq!(w.client.login().await.status, 200);

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
    assert_eq!(
        w.identity.last_auth_header.lock().unwrap().as_deref(),
        Some("Bearer acc-1")
    );
    assert_eq!(
        w.candidate.last_auth_header.lock().unwrap().as_deref(),
        Some("Bearer acc-1")
    );
}

#[tokio::test]
async fn paths_outside_the_allowlist_are_not_forwarded_to_any_service() {
    let w = world().await;
    w.client.login().await;
    for path in [
        "/api/persons-admin",
        "/api/internal/persons",
        "/api/tenants",
        "/api/admin/secret",
    ] {
        assert_eq!(
            w.client.send("GET", path, &[], None).await.status,
            404,
            "{path}"
        );
    }
    assert_eq!(w.identity.api_calls.load(Ordering::SeqCst), 0);
    assert_eq!(w.candidate.api_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_unavailable_service_only_breaks_its_own_routes() {
    // порт 9 (discard) закрыт: соединение отклоняется
    let w = world_with(Some("http://127.0.0.1:9".into())).await;
    assert_eq!(
        w.client.login().await.status,
        200,
        "login does not depend on candidate"
    );

    assert_eq!(
        w.client.send("GET", "/api/persons", &[], None).await.status,
        502
    );
    assert_eq!(
        w.client.send("GET", "/api/users", &[], None).await.status,
        200
    );
    let session = w.client.send("GET", "/bff/session", &[], None).await;
    assert_eq!(
        session.body["authenticated"], true,
        "session survives a backend outage"
    );
}

#[tokio::test]
async fn csrf_applies_to_every_service() {
    let w = world().await;
    let login = w.client.login().await;
    let csrf = login.body["csrf_token"].as_str().unwrap().to_string();
    let body = || Some(serde_json::json!({"full_name": "Иван"}));

    assert_eq!(
        w.client
            .send("POST", "/api/persons", &[], body())
            .await
            .status,
        403
    );
    assert_eq!(w.candidate.api_calls.load(Ordering::SeqCst), 0);
    let ok = w
        .client
        .send("POST", "/api/persons", &[("x-csrf-token", &csrf)], body())
        .await;
    assert_eq!(ok.status, 200);
}

#[tokio::test]
async fn a_revoked_token_is_refreshed_and_retried_against_the_other_service() {
    let w = world().await;
    w.client.login().await;
    w.identity.revoke_access("acc-1");

    let res = w.client.send("GET", "/api/persons", &[], None).await;
    assert_eq!(res.status, 200, "{}", res.raw);
    assert_eq!(w.identity.refresh_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        w.candidate.last_auth_header.lock().unwrap().as_deref(),
        Some("Bearer acc-2")
    );
}
