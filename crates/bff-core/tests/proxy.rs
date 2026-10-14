mod common;

use std::sync::atomic::Ordering;

use common::{COOKIE, harness, harness_with};
use serde_json::json;

async fn logged_in() -> (common::Harness, String) {
    let h = harness().await;
    let login = h.client.login().await;
    assert_eq!(login.status, 200);
    let csrf = login.body["csrf_token"].as_str().unwrap().to_string();
    (h, csrf)
}

#[tokio::test]
async fn proxy_attaches_the_bearer_token_and_relays_the_response() {
    let (h, _) = logged_in().await;
    let res = h.client.send("GET", "/api/users", &[], None).await;
    assert_eq!(res.status, 200, "{}", res.raw);
    assert_eq!(res.body, json!({"users": ["anna"]}));
    assert_eq!(
        h.upstream.last_auth_header.lock().unwrap().as_deref(),
        Some("Bearer acc-1")
    );
    // заголовки бэкенда фильтруются: чужой Set-Cookie не доходит до браузера
    assert!(res.headers.get("set-cookie").is_none());
    assert_eq!(res.headers.get("x-safe").unwrap(), "yes");
}

#[tokio::test]
async fn client_cannot_inject_its_own_authorization() {
    let (h, _) = logged_in().await;
    h.client
        .send(
            "GET",
            "/api/users",
            &[("authorization", "Bearer hacked")],
            None,
        )
        .await;
    assert_eq!(
        h.upstream.last_auth_header.lock().unwrap().as_deref(),
        Some("Bearer acc-1")
    );
}

#[tokio::test]
async fn unauthenticated_requests_never_reach_the_backend() {
    let h = harness().await;
    let res = h.client.send("GET", "/api/users", &[], None).await;
    assert_eq!(res.status, 401);
    assert_eq!(h.upstream.api_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn routes_outside_the_allowlist_are_404_and_not_forwarded() {
    let (h, _) = logged_in().await;
    for path in [
        "/api/admin/secret",
        "/api/users-admin",
        "/api/",
        "/api/auth/login",
    ] {
        let res = h.client.send("GET", path, &[], None).await;
        assert_eq!(res.status, 404, "{path}");
    }
    assert_eq!(h.upstream.api_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn path_traversal_cannot_escape_the_allowlist() {
    let (h, _) = logged_in().await;
    for path in [
        "/api/users/../admin/secret",
        "/api/users/%2e%2e/admin/secret",
        "/api/users/..%2fadmin/secret",
    ] {
        let res = h.client.send("GET", path, &[], None).await;
        assert_ne!(res.status, 200, "{path}");
    }
    assert_eq!(h.upstream.api_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn mutating_requests_need_a_matching_csrf_token() {
    let (h, csrf) = logged_in().await;
    let body = || Some(json!({"name": "x"}));

    let none = h.client.send("POST", "/api/users", &[], body()).await;
    assert_eq!(none.status, 403);
    let wrong = h
        .client
        .send("POST", "/api/users", &[("x-csrf-token", "nope")], body())
        .await;
    assert_eq!(wrong.status, 403);
    assert_eq!(h.upstream.api_calls.load(Ordering::SeqCst), 0);

    let ok = h
        .client
        .send("POST", "/api/users", &[("x-csrf-token", &csrf)], body())
        .await;
    assert_eq!(ok.status, 200);
    // безопасные методы не требуют заголовка
    assert_eq!(
        h.client.send("GET", "/api/users", &[], None).await.status,
        200
    );
}

#[tokio::test]
async fn expired_access_token_is_refreshed_transparently() {
    let h = harness().await;
    *h.upstream.access_ttl_secs.lock().unwrap() = 1; // меньше защитного запаса -> сразу устарел
    h.client.login().await;
    *h.upstream.access_ttl_secs.lock().unwrap() = 600;

    let res = h.client.send("GET", "/api/users", &[], None).await;
    assert_eq!(res.status, 200, "{}", res.raw);
    assert_eq!(h.upstream.refresh_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        h.upstream.last_auth_header.lock().unwrap().as_deref(),
        Some("Bearer acc-2")
    );
    // следующий запрос использует уже обновлённый токен
    h.client.send("GET", "/api/users", &[], None).await;
    assert_eq!(h.upstream.refresh_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn concurrent_requests_refresh_only_once() {
    let h = harness().await;
    *h.upstream.access_ttl_secs.lock().unwrap() = 1;
    h.client.login().await;
    *h.upstream.access_ttl_secs.lock().unwrap() = 600;

    let mut tasks = Vec::new();
    for _ in 0..5 {
        let c = h.client.clone();
        tasks.push(tokio::spawn(async move {
            c.send("GET", "/api/users", &[], None).await.status
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), 200);
    }
    assert_eq!(
        h.upstream.refresh_calls.load(Ordering::SeqCst),
        1,
        "parallel refresh would look like token theft to the identity service"
    );
}

#[tokio::test]
async fn backend_401_triggers_one_refresh_and_a_retry() {
    let (h, _) = logged_in().await;
    h.upstream.revoke_access("acc-1"); // токен отозван на стороне бэкенда

    let res = h.client.send("GET", "/api/users", &[], None).await;
    assert_eq!(res.status, 200, "{}", res.raw);
    assert_eq!(h.upstream.refresh_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        h.upstream.last_auth_header.lock().unwrap().as_deref(),
        Some("Bearer acc-2")
    );
}

#[tokio::test]
async fn failed_refresh_ends_the_session() {
    let h = harness().await;
    *h.upstream.access_ttl_secs.lock().unwrap() = 1;
    h.client.login().await;
    *h.upstream.refresh_status.lock().unwrap() = Some(401);

    let res = h.client.send("GET", "/api/users", &[], None).await;
    assert_eq!(res.status, 401);
    assert!(h.client.session_cookie_value().is_none(), "cookie cleared");
    let sc = res.headers.get("set-cookie").unwrap().to_str().unwrap();
    assert!(sc.starts_with(&format!("{COOKIE}=;")) && sc.contains("Max-Age=0"));
    // сессия удалена из хранилища, а не только из браузера
    let after = h.client.send("GET", "/bff/session", &[], None).await;
    assert_eq!(after.body["authenticated"], false);
}

#[tokio::test]
async fn suspended_tenant_refresh_ends_the_session_with_401() {
    let h = harness().await;
    *h.upstream.access_ttl_secs.lock().unwrap() = 1;
    h.client.login().await;
    let id = h.client.session_cookie_value().unwrap();
    *h.upstream.refresh_status.lock().unwrap() = Some(403);

    let res = h.client.send("GET", "/api/users", &[], None).await;
    assert_eq!(res.status, 401);
    assert_eq!(h.store.get(&id).await.unwrap(), None);
}

#[tokio::test]
async fn upstream_outage_during_refresh_keeps_the_session() {
    let h = harness().await;
    *h.upstream.access_ttl_secs.lock().unwrap() = 1;
    h.client.login().await;
    let id = h.client.session_cookie_value().unwrap();
    *h.upstream.refresh_status.lock().unwrap() = Some(503);

    let res = h.client.send("GET", "/api/users", &[], None).await;
    assert_eq!(res.status, 502);
    assert!(h.store.get(&id).await.unwrap().is_some());
}

#[tokio::test]
async fn login_transform_rewrites_the_credentials() {
    let h = harness_with(|cfg| {
        cfg.login_transform = Some(std::sync::Arc::new(|headers, mut body| {
            let tenant = headers
                .get("x-test-tenant")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(platform_core::error::ApiError::unauthorized)?;
            body["tenant"] = json!(tenant);
            Ok(body)
        }));
    })
    .await;
    let res = h
        .client
        .send(
            "POST",
            "/bff/login",
            &[("x-csrf-token", "login"), ("x-test-tenant", "acme")],
            Some(json!({"email": "a@b.io", "password": "good-password"})),
        )
        .await;
    assert_eq!(res.status, 200);
    assert_eq!(
        h.upstream.last_login_body.lock().unwrap().as_ref().unwrap()["tenant"],
        "acme"
    );

    let denied = h.client.send(
        "POST",
        "/bff/login",
        &[("x-csrf-token", "login")],
        Some(json!({"email": "a@b.io", "password": "good-password"})),
    );
    assert_eq!(denied.await.status, 401);
}
