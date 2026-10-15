mod common;

use common::setup;
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::json;

#[tokio::test]
async fn without_a_token_every_endpoint_answers_401() {
    let app = setup().await;
    for (method, uri) in [
        ("GET", "/clients"),
        ("POST", "/clients"),
        ("GET", "/clients/00000000-0000-0000-0000-000000000001"),
        ("PATCH", "/contacts/00000000-0000-0000-0000-000000000001"),
        ("GET", "/positions"),
        ("POST", "/positions"),
    ] {
        let (s, _) = app.send(method, uri, None, Some(json!({}))).await;
        assert_eq!(s, 401, "{method} {uri}");
    }
    app.close().await;
}

#[tokio::test]
async fn a_viewer_reads_but_cannot_write() {
    let app = setup().await;
    let tenant = TenantId::new();
    let recruiter = app.token(tenant, &[Role::Recruiter]);
    let viewer = app.token(tenant, &[Role::Viewer]);
    let (_, client) = app
        .send(
            "POST",
            "/clients",
            Some(&recruiter),
            Some(json!({"name": "Acme"})),
        )
        .await;
    let id = client["id"].as_str().unwrap();
    let (_, pos) = app
        .send(
            "POST",
            "/positions",
            Some(&recruiter),
            Some(json!({"name": "HR"})),
        )
        .await;
    let (_, contact) = app
        .send(
            "POST",
            &format!("/clients/{id}/contacts"),
            Some(&recruiter),
            Some(json!({"full_name": "М"})),
        )
        .await;
    let cid = contact["id"].as_str().unwrap();

    for uri in ["/clients", &format!("/clients/{id}"), "/positions"] {
        let (s, _) = app.send("GET", uri, Some(&viewer), None).await;
        assert_eq!(s, 200, "{uri}");
    }
    for (method, uri, body) in [
        ("POST", "/clients".to_string(), json!({"name": "Новый"})),
        ("PATCH", format!("/clients/{id}"), json!({"archived": true})),
        (
            "POST",
            format!("/clients/{id}/contacts"),
            json!({"full_name": "Н"}),
        ),
        (
            "PATCH",
            format!("/contacts/{cid}"),
            json!({"full_name": "Н"}),
        ),
        ("DELETE", format!("/contacts/{cid}"), json!({})),
        ("POST", "/positions".to_string(), json!({"name": "Новая"})),
        (
            "PATCH",
            format!("/positions/{}", pos["id"].as_str().unwrap()),
            json!({"name": "Н"}),
        ),
        (
            "DELETE",
            format!("/positions/{}", pos["id"].as_str().unwrap()),
            json!({}),
        ),
    ] {
        let (s, _) = app.send(method, &uri, Some(&viewer), Some(body)).await;
        assert_eq!(s, 403, "{method} {uri}");
    }
    app.close().await;
}

#[tokio::test]
async fn tokens_without_a_tenant_or_for_the_platform_are_rejected() {
    let app = setup().await;
    let (s, _) = app
        .send("GET", "/clients", Some(&app.tenantless_token()), None)
        .await;
    assert_eq!(s, 403);
    let (s, _) = app
        .send("GET", "/clients", Some(&app.platform_token()), None)
        .await;
    assert_eq!(s, 403);
    app.close().await;
}
