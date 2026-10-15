mod common;

use std::sync::atomic::Ordering;

use bff_core::parse_cors_origins;
use common::harness_with;

const ALLOWED: &str = "http://localhost:3000";

async fn with_cors() -> common::Harness {
    harness_with(|c| c.cors_origins = vec![ALLOWED.to_string()]).await
}

#[tokio::test]
async fn allowed_origin_gets_credentials_headers() {
    let h = with_cors().await;
    let res = h
        .client
        .send("GET", "/bff/session", &[("origin", ALLOWED)], None)
        .await;
    assert_eq!(res.status, 200);
    assert_eq!(res.headers["access-control-allow-origin"], ALLOWED);
    assert_eq!(res.headers["access-control-allow-credentials"], "true");
    assert!(
        res.headers["vary"]
            .to_str()
            .unwrap()
            .to_lowercase()
            .contains("origin")
    );
}

#[tokio::test]
async fn preflight_is_answered_without_a_session_or_csrf() {
    let h = with_cors().await;
    let res = h
        .client
        .send(
            "OPTIONS",
            "/api/users",
            &[
                ("origin", ALLOWED),
                ("access-control-request-method", "POST"),
                (
                    "access-control-request-headers",
                    "content-type,x-csrf-token",
                ),
            ],
            None,
        )
        .await;
    assert!(res.status == 200 || res.status == 204, "{}", res.status);
    assert_eq!(res.headers["access-control-allow-origin"], ALLOWED);
    assert_eq!(res.headers["access-control-allow-credentials"], "true");
    let methods = res.headers["access-control-allow-methods"]
        .to_str()
        .unwrap();
    for m in ["GET", "POST", "PUT", "PATCH", "DELETE"] {
        assert!(methods.contains(m), "{m} in {methods}");
    }
    let headers = res.headers["access-control-allow-headers"]
        .to_str()
        .unwrap()
        .to_lowercase();
    assert!(headers.contains("content-type") && headers.contains("x-csrf-token"));
    assert!(res.headers.get("set-cookie").is_none());
    assert_eq!(
        h.upstream.api_calls.load(Ordering::SeqCst),
        0,
        "the backend never sees a preflight"
    );
}

#[tokio::test]
async fn foreign_origin_gets_nothing() {
    let h = with_cors().await;
    for (method, extra) in [
        ("GET", vec![("origin", "http://evil.example")]),
        (
            "OPTIONS",
            vec![
                ("origin", "http://evil.example"),
                ("access-control-request-method", "POST"),
            ],
        ),
    ] {
        let res = h.client.send(method, "/bff/session", &extra, None).await;
        assert!(
            res.headers.get("access-control-allow-origin").is_none(),
            "{method}"
        );
        assert!(
            res.headers
                .get("access-control-allow-credentials")
                .is_none()
        );
    }
}

#[tokio::test]
async fn a_lookalike_origin_is_not_allowed() {
    let h = with_cors().await;
    for origin in [
        "http://localhost:3000.evil.example",
        "http://localhost:3001",
        "https://localhost:3000",
        "HTTP://LOCALHOST:3000",
    ] {
        let res = h
            .client
            .send("GET", "/bff/session", &[("origin", origin)], None)
            .await;
        assert!(
            res.headers.get("access-control-allow-origin").is_none(),
            "{origin}"
        );
    }
}

#[tokio::test]
async fn csrf_is_still_required_for_cross_origin_writes() {
    let h = with_cors().await;
    h.client.login().await;
    let res = h
        .client
        .send(
            "POST",
            "/api/users",
            &[("origin", ALLOWED)],
            Some(serde_json::json!({})),
        )
        .await;
    assert_eq!(res.status, 403);
    assert_eq!(
        res.headers["access-control-allow-origin"], ALLOWED,
        "the browser must be able to read the error"
    );
    assert_eq!(h.upstream.api_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn errors_from_the_proxy_carry_cors_headers_too() {
    let h = with_cors().await;
    let res = h
        .client
        .send("GET", "/api/users", &[("origin", ALLOWED)], None)
        .await;
    assert_eq!(res.status, 401);
    assert_eq!(res.headers["access-control-allow-origin"], ALLOWED);
}

#[tokio::test]
async fn without_configuration_cors_is_off() {
    let h = harness_with(|_| {}).await;
    let res = h
        .client
        .send("GET", "/bff/session", &[("origin", ALLOWED)], None)
        .await;
    assert!(res.headers.get("access-control-allow-origin").is_none());
    let pre = h
        .client
        .send(
            "OPTIONS",
            "/api/users",
            &[
                ("origin", ALLOWED),
                ("access-control-request-method", "POST"),
            ],
            None,
        )
        .await;
    assert!(pre.headers.get("access-control-allow-origin").is_none());
}

// ---------- разбор настройки ----------

#[test]
fn origins_are_split_trimmed_and_normalized() {
    assert_eq!(
        parse_cors_origins(" http://localhost:3000 , https://crm.example.com/ ").unwrap(),
        ["http://localhost:3000", "https://crm.example.com"]
    );
    assert!(parse_cors_origins("").unwrap().is_empty());
    assert!(parse_cors_origins("  ").unwrap().is_empty());
}

#[test]
fn wildcards_and_garbage_are_rejected() {
    for bad in [
        "*",
        "http://a.example,*",
        "http://a.example,,http://b.example",
        "localhost:3000",
        "ftp://a.example",
        "http://a.example/path",
        "null",
    ] {
        assert!(parse_cors_origins(bad).is_err(), "{bad}");
    }
}
