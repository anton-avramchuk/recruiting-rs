mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};

async fn create(app: &App, token: &str, name: &str) -> Value {
    let (s, v) = app
        .send("POST", "/clients", Some(token), Some(json!({"name": name})))
        .await;
    assert_eq!(s, 201, "{v}");
    v
}

#[tokio::test]
async fn a_client_is_created_trimmed_and_listed() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let client = create(&app, &token, "  Acme   Corp ").await;
    assert_eq!(client["name"], "Acme Corp");
    assert_eq!(client["archived"], false);
    assert_eq!(client["contacts_count"], 0);

    let (s, list) = app.send("GET", "/clients", Some(&token), None).await;
    assert_eq!(s, 200);
    assert_eq!(list.as_array().unwrap().len(), 1);
    app.close().await;
}

#[tokio::test]
async fn validation_errors_name_the_field() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    for body in [
        json!({"name": "   "}),
        json!({"name": "A", "note": "я".repeat(2001)}),
    ] {
        let (s, v) = app.send("POST", "/clients", Some(&token), Some(body)).await;
        assert_eq!(s, 400, "{v}");
        assert!(v["field"] == "name" || v["field"] == "note", "{v}");
    }
    app.close().await;
}

#[tokio::test]
async fn a_duplicate_name_conflicts_and_points_to_the_existing_client() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let first = create(&app, &token, "Acme Corp").await;
    let (s, v) = app
        .send(
            "POST",
            "/clients",
            Some(&token),
            Some(json!({"name": " acme  corp "})),
        )
        .await;
    assert_eq!(s, 409, "{v}");
    assert_eq!(v["duplicate"]["id"], first["id"]);
    let (_, list) = app.send("GET", "/clients", Some(&token), None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    app.close().await;
}

#[tokio::test]
async fn the_same_name_is_fine_in_another_organization() {
    let app = setup().await;
    let a = app.token(TenantId::new(), &[Role::Recruiter]);
    let b = app.token(TenantId::new(), &[Role::Recruiter]);
    create(&app, &a, "Acme").await;
    create(&app, &b, "Acme").await;
    app.close().await;
}

#[tokio::test]
async fn archiving_hides_a_client_from_the_default_list_only() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let client = create(&app, &token, "Acme").await;
    create(&app, &token, "Globex").await;
    let id = client["id"].as_str().unwrap();

    let (s, v) = app
        .send(
            "PATCH",
            &format!("/clients/{id}"),
            Some(&token),
            Some(json!({"archived": true})),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["archived"], true);

    let (_, active) = app.send("GET", "/clients", Some(&token), None).await;
    assert_eq!(active.as_array().unwrap().len(), 1);
    let (_, all) = app
        .send("GET", "/clients?archived=true", Some(&token), None)
        .await;
    assert_eq!(all.as_array().unwrap().len(), 2);
    // активные идут раньше архивных
    assert_eq!(all[0]["name"], "Globex");
    let (s, card) = app
        .send("GET", &format!("/clients/{id}"), Some(&token), None)
        .await;
    assert_eq!(s, 200);
    assert_eq!(card["client"]["archived"], true);

    let (s, back) = app
        .send(
            "PATCH",
            &format!("/clients/{id}"),
            Some(&token),
            Some(json!({"archived": false})),
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(back["archived"], false);
    app.close().await;
}

#[tokio::test]
async fn search_matches_a_substring_without_case_and_wildcards_are_literal() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    create(&app, &token, "Acme Corp").await;
    create(&app, &token, "Globex").await;
    create(&app, &token, "100% Rabota").await;

    let (_, found) = app.send("GET", "/clients?q=ACM", Some(&token), None).await;
    assert_eq!(found.as_array().unwrap().len(), 1);
    let (_, percent) = app.send("GET", "/clients?q=%25", Some(&token), None).await;
    assert_eq!(percent.as_array().unwrap().len(), 1, "{percent}");
    assert_eq!(percent[0]["name"], "100% Rabota");
    app.close().await;
}

#[tokio::test]
async fn renaming_into_an_existing_name_conflicts() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    create(&app, &token, "Acme").await;
    let globex = create(&app, &token, "Globex").await;
    let id = globex["id"].as_str().unwrap();

    let (s, _) = app
        .send(
            "PATCH",
            &format!("/clients/{id}"),
            Some(&token),
            Some(json!({"name": "ACME"})),
        )
        .await;
    assert_eq!(s, 409);
    // а то же название у самого себя допустимо
    let (s, v) = app
        .send(
            "PATCH",
            &format!("/clients/{id}"),
            Some(&token),
            Some(json!({"name": "GLOBEX", "note": "важный"})),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["note"], "важный");
    let (s, v) = app
        .send(
            "PATCH",
            &format!("/clients/{id}"),
            Some(&token),
            Some(json!({"note": null})),
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(v["note"], Value::Null);
    app.close().await;
}

#[tokio::test]
async fn another_organizations_client_is_not_found() {
    let app = setup().await;
    let a = app.token(TenantId::new(), &[Role::Recruiter]);
    let b = app.token(TenantId::new(), &[Role::Recruiter]);
    let client = create(&app, &a, "Acme").await;
    let id = client["id"].as_str().unwrap();
    let (s, _) = app
        .send("GET", &format!("/clients/{id}"), Some(&b), None)
        .await;
    assert_eq!(s, 404);
    let (s, _) = app
        .send(
            "PATCH",
            &format!("/clients/{id}"),
            Some(&b),
            Some(json!({"archived": true})),
        )
        .await;
    assert_eq!(s, 404);
    let (s, _) = app
        .send(
            "POST",
            &format!("/clients/{id}/contacts"),
            Some(&b),
            Some(json!({"full_name": "X"})),
        )
        .await;
    assert_eq!(s, 404);
    let (_, list) = app.send("GET", "/clients", Some(&b), None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
    app.close().await;
}
