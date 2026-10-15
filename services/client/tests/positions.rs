mod common;

use common::setup;
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::json;

#[tokio::test]
async fn positions_are_created_listed_and_unique_ignoring_case() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let (s, first) = app
        .send(
            "POST",
            "/positions",
            Some(&token),
            Some(json!({"name": " HR  директор "})),
        )
        .await;
    assert_eq!(s, 201);
    assert_eq!(first["name"], "HR директор");
    let (s, dup) = app
        .send(
            "POST",
            "/positions",
            Some(&token),
            Some(json!({"name": "hr ДИРЕКТОР"})),
        )
        .await;
    assert_eq!(s, 409, "{dup}");
    assert_eq!(dup["duplicate"]["id"], first["id"]);
    let (s, v) = app
        .send(
            "POST",
            "/positions",
            Some(&token),
            Some(json!({"name": ""})),
        )
        .await;
    assert_eq!((s, v["field"].as_str()), (400, Some("name")));

    app.send(
        "POST",
        "/positions",
        Some(&token),
        Some(json!({"name": "CTO"})),
    )
    .await;
    let (_, list) = app.send("GET", "/positions", Some(&token), None).await;
    let names: Vec<_> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["CTO", "HR директор"]);
    app.close().await;
}

#[tokio::test]
async fn renaming_shows_everywhere_and_cannot_collide() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let (_, a) = app
        .send(
            "POST",
            "/positions",
            Some(&token),
            Some(json!({"name": "Директор по персоналу"})),
        )
        .await;
    let (_, b) = app
        .send(
            "POST",
            "/positions",
            Some(&token),
            Some(json!({"name": "CTO"})),
        )
        .await;
    let a_id = a["id"].as_str().unwrap();

    let (_, client) = app
        .send(
            "POST",
            "/clients",
            Some(&token),
            Some(json!({"name": "Acme"})),
        )
        .await;
    let client_id = client["id"].as_str().unwrap();
    app.send(
        "POST",
        &format!("/clients/{client_id}/contacts"),
        Some(&token),
        Some(json!({"full_name": "Мария", "position_id": a_id})),
    )
    .await;

    let (s, v) = app
        .send(
            "PATCH",
            &format!("/positions/{a_id}"),
            Some(&token),
            Some(json!({"name": "HR-директор"})),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (_, card) = app
        .send("GET", &format!("/clients/{client_id}"), Some(&token), None)
        .await;
    assert_eq!(card["contacts"][0]["position"]["name"], "HR-директор");

    let (s, _) = app
        .send(
            "PATCH",
            &format!("/positions/{a_id}"),
            Some(&token),
            Some(json!({"name": "cto"})),
        )
        .await;
    assert_eq!(s, 409);
    let _ = b;
    app.close().await;
}

#[tokio::test]
async fn a_position_in_use_cannot_be_deleted_but_a_free_one_can() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let (_, used) = app
        .send(
            "POST",
            "/positions",
            Some(&token),
            Some(json!({"name": "Используемая"})),
        )
        .await;
    let (_, free) = app
        .send(
            "POST",
            "/positions",
            Some(&token),
            Some(json!({"name": "Свободная"})),
        )
        .await;
    let (_, client) = app
        .send(
            "POST",
            "/clients",
            Some(&token),
            Some(json!({"name": "Acme"})),
        )
        .await;
    app.send(
        "POST",
        &format!("/clients/{}/contacts", client["id"].as_str().unwrap()),
        Some(&token),
        Some(json!({"full_name": "Мария", "position_id": used["id"]})),
    )
    .await;

    let (s, v) = app
        .send(
            "DELETE",
            &format!("/positions/{}", used["id"].as_str().unwrap()),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(s, 409, "{v}");
    let (s, _) = app
        .send(
            "DELETE",
            &format!("/positions/{}", free["id"].as_str().unwrap()),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(s, 204);
    let (_, list) = app.send("GET", "/positions", Some(&token), None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    app.close().await;
}

#[tokio::test]
async fn another_organizations_positions_are_invisible() {
    let app = setup().await;
    let a = app.token(TenantId::new(), &[Role::Recruiter]);
    let b = app.token(TenantId::new(), &[Role::Recruiter]);
    let (_, pos) = app
        .send(
            "POST",
            "/positions",
            Some(&a),
            Some(json!({"name": "Директор"})),
        )
        .await;
    let id = pos["id"].as_str().unwrap();
    let (_, list) = app.send("GET", "/positions", Some(&b), None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
    let (s, _) = app
        .send(
            "PATCH",
            &format!("/positions/{id}"),
            Some(&b),
            Some(json!({"name": "X"})),
        )
        .await;
    assert_eq!(s, 404);
    let (s, _) = app
        .send("DELETE", &format!("/positions/{id}"), Some(&b), None)
        .await;
    assert_eq!(s, 404);
    app.close().await;
}
