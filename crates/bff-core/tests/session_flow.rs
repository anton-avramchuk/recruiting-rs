mod common;

use common::{COOKIE, harness};
use serde_json::json;

#[tokio::test]
async fn login_sets_a_hardened_cookie_and_never_exposes_tokens() {
    let h = harness().await;
    let res = h.client.login().await;
    assert_eq!(res.status, 200, "{}", res.raw);

    let set_cookie = res.headers.get("set-cookie").unwrap().to_str().unwrap();
    assert!(set_cookie.starts_with(&format!("{COOKIE}=")));
    for attr in ["HttpOnly", "Secure", "SameSite=Lax", "Path=/"] {
        assert!(
            set_cookie.contains(attr),
            "cookie lacks {attr}: {set_cookie}"
        );
    }
    // ни токены, ни их производные не попадают в ответ браузеру
    let everything = format!("{:?}{}", res.headers, res.raw);
    assert!(!everything.contains("acc-1"), "{everything}");
    assert!(!everything.contains("ref-1"), "{everything}");
    assert!(res.body["csrf_token"].as_str().unwrap().len() >= 20);
    // значение cookie - идентификатор сессии, а не токен
    assert_ne!(h.client.session_cookie_value().unwrap(), "acc-1");
}

#[tokio::test]
async fn user_preview_carries_login_metadata_but_never_the_password() {
    let h = harness().await;
    let login = h.client.login().await;
    assert_eq!(login.body["user"]["email"], "a@b.io");
    assert!(!login.raw.contains("good-password"));

    let session = h.client.send("GET", "/bff/session", &[], None).await;
    assert_eq!(session.body["user"]["email"], "a@b.io");
    assert!(!session.raw.contains("good-password"));
}

#[tokio::test]
async fn rejected_credentials_give_401_without_a_cookie() {
    let h = harness().await;
    let res = h
        .client
        .send(
            "POST",
            "/bff/login",
            &[("x-csrf-token", "login")],
            Some(json!({"email": "a@b.io", "password": "bad"})),
        )
        .await;
    assert_eq!(res.status, 401);
    assert!(res.headers.get("set-cookie").is_none());
    assert!(h.client.session_cookie_value().is_none());
}

#[tokio::test]
async fn login_requires_the_csrf_header() {
    let h = harness().await;
    let res = h
        .client
        .send(
            "POST",
            "/bff/login",
            &[],
            Some(json!({"email": "a@b.io", "password": "good-password"})),
        )
        .await;
    assert_eq!(res.status, 403);
    assert!(h.client.session_cookie_value().is_none());
}

#[tokio::test]
async fn session_endpoint_reports_state_and_user_preview() {
    let h = harness().await;
    let anon = h.client.send("GET", "/bff/session", &[], None).await;
    assert_eq!(
        (anon.status, &anon.body["authenticated"]),
        (200, &json!(false))
    );

    let login = h.client.login().await;
    let res = h.client.send("GET", "/bff/session", &[], None).await;
    assert_eq!(res.status, 200);
    assert_eq!(res.body["authenticated"], true);
    assert_eq!(res.body["csrf_token"], login.body["csrf_token"]);
}

#[tokio::test]
async fn logout_revokes_upstream_and_destroys_the_session() {
    let h = harness().await;
    let login = h.client.login().await;
    let csrf = login.body["csrf_token"].as_str().unwrap().to_string();

    let res = h
        .client
        .send("POST", "/bff/logout", &[("x-csrf-token", &csrf)], None)
        .await;
    assert_eq!(res.status, 204);
    assert_eq!(*h.upstream.logout_calls.lock().unwrap(), ["ref-1"]);
    assert!(h.client.session_cookie_value().is_none(), "cookie cleared");
    let set_cookie = res.headers.get("set-cookie").unwrap().to_str().unwrap();
    assert!(set_cookie.contains("Max-Age=0"));

    let after = h.client.send("GET", "/bff/session", &[], None).await;
    assert_eq!(after.body["authenticated"], false);
}

#[tokio::test]
async fn logout_requires_a_valid_csrf_token() {
    let h = harness().await;
    h.client.login().await;
    let res = h
        .client
        .send("POST", "/bff/logout", &[("x-csrf-token", "wrong")], None)
        .await;
    assert_eq!(res.status, 403);
    assert!(h.upstream.logout_calls.lock().unwrap().is_empty());
    let still = h.client.send("GET", "/bff/session", &[], None).await;
    assert_eq!(still.body["authenticated"], true);
}

#[tokio::test]
async fn a_cookie_with_another_name_is_not_a_session() {
    let h = harness().await;
    h.client.login().await;
    let id = h.client.session_cookie_value().unwrap();
    // тот же идентификатор под именем cookie другого слоя
    *h.client.cookie.lock().unwrap() = Some(format!("admin_session={id}"));
    let res = h.client.send("GET", "/bff/session", &[], None).await;
    assert_eq!(res.body["authenticated"], false);
}

#[tokio::test]
async fn unknown_session_id_is_unauthenticated() {
    let h = harness().await;
    *h.client.cookie.lock().unwrap() = Some(format!("{COOKIE}=forged-id"));
    let res = h.client.send("GET", "/bff/session", &[], None).await;
    assert_eq!(res.body["authenticated"], false);
}
