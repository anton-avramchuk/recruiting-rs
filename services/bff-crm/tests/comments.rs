use std::sync::{Arc, atomic::Ordering};

use bff_core::{
    MemorySessionStore,
    testing::{Client, Upstream},
};
use bff_crm::{CrmConfig, build};
use serde_json::json;

struct World {
    comments: Upstream,
    others: Vec<Upstream>,
    client: Client,
    csrf: String,
}

async fn world(comment_url: Option<String>) -> World {
    let identity = Upstream::new();
    let shared = |u: &Upstream| Upstream {
        valid_access: u.valid_access.clone(),
        ..Upstream::new()
    };
    let (candidate, recruitment, clients, comments) = (
        shared(&identity),
        shared(&identity),
        shared(&identity),
        shared(&identity),
    );
    let cfg = CrmConfig {
        identity_url: identity.serve().await,
        candidate_url: candidate.serve().await,
        recruitment_url: recruitment.serve().await,
        client_url: clients.serve().await,
        comment_url: match comment_url {
            Some(url) => url,
            None => comments.serve().await,
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
        comments,
        others: vec![candidate, recruitment, clients],
        client,
        csrf,
    }
}

#[tokio::test]
async fn comment_routes_reach_only_the_comment_service() {
    let w = world(None).await;
    for path in [
        "/api/comments?target_type=person&target_id=1",
        "/api/comments/1",
    ] {
        let res = w.client.send("GET", path, &[], None).await;
        assert_eq!(res.status, 200, "{path}");
    }
    assert_eq!(w.comments.api_calls.load(Ordering::SeqCst), 2);
    for other in &w.others {
        assert_eq!(other.api_calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn comment_writes_need_the_csrf_token() {
    let w = world(None).await;
    for (method, path) in [
        ("POST", "/api/comments"),
        ("PATCH", "/api/comments/1"),
        ("DELETE", "/api/comments/1"),
    ] {
        let res = w.client.send(method, path, &[], Some(json!({}))).await;
        assert_eq!(res.status, 403, "{method} {path}");
    }
    assert_eq!(w.comments.api_calls.load(Ordering::SeqCst), 0);
    let ok = w
        .client
        .send(
            "POST",
            "/api/comments",
            &[("x-csrf-token", &w.csrf)],
            Some(json!({"target_type": "person", "target_id": "x", "body": "текст"})),
        )
        .await;
    assert_eq!(ok.status, 200);
}

#[tokio::test]
async fn lookalikes_stay_closed() {
    let w = world(None).await;
    for path in [
        "/api/comments-export",
        "/api/commentsx",
        "/api/internal/comments",
    ] {
        for method in ["GET", "POST"] {
            let res = w
                .client
                .send(method, path, &[("x-csrf-token", &w.csrf)], None)
                .await;
            assert_eq!(res.status, 404, "{method} {path}");
        }
    }
    assert_eq!(w.comments.api_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_dead_comment_service_does_not_break_the_rest() {
    let w = world(Some("http://127.0.0.1:9".into())).await;
    let res = w
        .client
        .send(
            "GET",
            "/api/comments?target_type=person&target_id=1",
            &[],
            None,
        )
        .await;
    assert_eq!(res.status, 502);
    for path in [
        "/api/users",
        "/api/persons",
        "/api/vacancies",
        "/api/clients",
    ] {
        assert_eq!(
            w.client.send("GET", path, &[], None).await.status,
            200,
            "{path}"
        );
    }
}
