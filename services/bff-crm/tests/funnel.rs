use std::sync::{Arc, atomic::Ordering};

use bff_core::{
    MemorySessionStore,
    testing::{Client, Upstream},
};
use bff_crm::{CrmConfig, build};
use serde_json::json;

struct World {
    candidate: Upstream,
    recruitment: Upstream,
    client: Client,
    csrf: String,
}

async fn world(recruitment_url: Option<String>) -> World {
    let identity = Upstream::new();
    let candidate = Upstream {
        valid_access: identity.valid_access.clone(),
        ..Upstream::new()
    };
    let recruitment = Upstream {
        valid_access: identity.valid_access.clone(),
        ..Upstream::new()
    };
    let cfg = CrmConfig {
        identity_url: identity.serve().await,
        candidate_url: candidate.serve().await,
        cors_origins: Vec::new(),
        client_url: "http://127.0.0.1:9".into(),
        comment_url: "http://127.0.0.1:9".into(),
        recruitment_url: match recruitment_url {
            Some(url) => url,
            None => recruitment.serve().await,
        },
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
        candidate,
        recruitment,
        client,
        csrf,
    }
}

#[tokio::test]
async fn funnel_routes_reach_the_recruitment_service() {
    let w = world(None).await;
    for path in [
        "/api/vacancies",
        "/api/vacancies/1",
        "/api/applications",
        "/api/applications/1/stage",
    ] {
        let res = w.client.send("GET", path, &[], None).await;
        assert_eq!(res.status, 200, "{path}");
    }
    assert_eq!(w.recruitment.api_calls.load(Ordering::SeqCst), 4);
    assert_eq!(w.candidate.api_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn the_people_of_a_client_come_from_the_recruitment_service() {
    let w = world(None).await;
    let res = w
        .client
        .send("GET", "/api/applications/people?client_id=1", &[], None)
        .await;
    assert_eq!(res.status, 200);
    assert_eq!(w.recruitment.api_calls.load(Ordering::SeqCst), 1);
    assert_eq!(w.candidate.api_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn funnel_writes_need_the_csrf_token() {
    let w = world(None).await;
    let body = || Some(json!({"stage": "hr"}));
    for method in ["POST", "PATCH"] {
        let res = w
            .client
            .send(method, "/api/applications/1/stage", &[], body())
            .await;
        assert_eq!(res.status, 403, "{method}");
    }
    assert_eq!(w.recruitment.api_calls.load(Ordering::SeqCst), 0);
    let ok = w
        .client
        .send(
            "POST",
            "/api/applications/1/stage",
            &[("x-csrf-token", &w.csrf)],
            body(),
        )
        .await;
    assert_eq!(ok.status, 200);
}

#[tokio::test]
async fn internal_and_service_paths_of_the_funnel_stay_closed() {
    let w = world(None).await;
    for path in [
        "/api/internal/applications",
        "/api/vacancies-export",
        "/api/applications-export",
        "/api/people",
    ] {
        for method in ["GET", "POST"] {
            let res = w
                .client
                .send(method, path, &[("x-csrf-token", &w.csrf)], None)
                .await;
            assert_eq!(res.status, 404, "{method} {path}");
        }
    }
    assert_eq!(w.recruitment.api_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_dead_funnel_service_does_not_break_the_rest() {
    let w = world(Some("http://127.0.0.1:9".into())).await;
    let res = w.client.send("GET", "/api/vacancies", &[], None).await;
    assert_eq!(res.status, 502);
    assert_eq!(
        w.client.send("GET", "/api/users", &[], None).await.status,
        200
    );
    assert_eq!(
        w.client.send("GET", "/api/persons", &[], None).await.status,
        200
    );
}

#[tokio::test]
async fn overview_and_interviews_reach_the_recruitment_service() {
    let w = world(None).await;
    for path in [
        "/api/overview",
        "/api/interviews?from=2026-03-10T00:00:00Z&to=2026-03-11T00:00:00Z",
    ] {
        let res = w.client.send("GET", path, &[], None).await;
        assert_eq!(res.status, 200, "{path}");
    }
    assert_eq!(w.recruitment.api_calls.load(Ordering::SeqCst), 2);
    assert_eq!(w.candidate.api_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn overview_lookalike_paths_stay_closed() {
    let w = world(None).await;
    for path in [
        "/api/overview-export",
        "/api/interviews-export",
        "/api/overviews",
    ] {
        let res = w.client.send("GET", path, &[], None).await;
        assert_eq!(res.status, 404, "{path}");
    }
    assert_eq!(w.recruitment.api_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn analytics_reaches_the_recruitment_service_and_lookalikes_stay_closed() {
    let w = world(None).await;
    let res = w
        .client
        .send("GET", "/api/analytics?days=90", &[], None)
        .await;
    assert_eq!(res.status, 200);
    assert_eq!(w.recruitment.api_calls.load(Ordering::SeqCst), 1);
    for path in ["/api/analytics-export", "/api/analyticsx"] {
        let res = w.client.send("GET", path, &[], None).await;
        assert_eq!(res.status, 404, "{path}");
    }
    assert_eq!(w.recruitment.api_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn grid_parameters_reach_the_recruitment_service_unchanged() {
    let w = world(None).await;
    let query = "page=1&page_size=20&sort=stage_changed_at%3Aasc&filter=%7B%22field%22%3A%22overdue%22%2C%22operator%22%3A%22eq%22%2C%22value%22%3Atrue%7D";
    let res = w
        .client
        .send("GET", &format!("/api/applications/grid?{query}"), &[], None)
        .await;
    assert_eq!(res.status, 200);
    assert_eq!(
        w.recruitment.last_query.lock().unwrap().as_deref(),
        Some(query)
    );
}
