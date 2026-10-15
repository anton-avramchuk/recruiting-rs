mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};

async fn client(app: &App, token: &str, name: &str) -> String {
    let (s, v) = app
        .send("POST", "/clients", Some(token), Some(json!({"name": name})))
        .await;
    assert_eq!(s, 201, "{v}");
    v["id"].as_str().unwrap().to_string()
}

async fn position(app: &App, token: &str, name: &str) -> String {
    let (s, v) = app
        .send(
            "POST",
            "/positions",
            Some(token),
            Some(json!({"name": name})),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    v["id"].as_str().unwrap().to_string()
}

async fn contact(app: &App, token: &str, client: &str, body: Value) -> (u16, Value) {
    app.send(
        "POST",
        &format!("/clients/{client}/contacts"),
        Some(token),
        Some(body),
    )
    .await
}

#[tokio::test]
async fn a_client_holds_several_contacts_with_positions() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let acme = client(&app, &token, "Acme").await;
    let hr = position(&app, &token, "HR-директор").await;
    let cto = position(&app, &token, "CTO").await;

    for (name, pos) in [
        ("Мария Петрова", &hr),
        ("Иван Орлов", &cto),
        ("Без должности", &String::new()),
    ] {
        let mut body = json!({"full_name": name, "email": "m@acme.io"});
        if !pos.is_empty() {
            body["position_id"] = json!(pos);
        }
        let (s, v) = contact(&app, &token, &acme, body).await;
        assert_eq!(s, 201, "{v}");
    }

    let (s, card) = app
        .send("GET", &format!("/clients/{acme}"), Some(&token), None)
        .await;
    assert_eq!(s, 200);
    let contacts = card["contacts"].as_array().unwrap();
    assert_eq!(contacts.len(), 3);
    assert_eq!(contacts[0]["position"]["name"], "HR-директор");
    assert_eq!(contacts[1]["position"]["name"], "CTO");
    assert_eq!(contacts[2]["position"], Value::Null);
    assert_eq!(card["client"]["contacts_count"], 3);
    app.close().await;
}

#[tokio::test]
async fn contact_validation_names_the_field() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let acme = client(&app, &token, "Acme").await;

    let (s, v) = contact(&app, &token, &acme, json!({"full_name": " "})).await;
    assert_eq!((s, v["field"].as_str()), (400, Some("full_name")));
    let (s, v) = contact(
        &app,
        &token,
        &acme,
        json!({"full_name": "А", "email": "не почта"}),
    )
    .await;
    assert_eq!((s, v["field"].as_str()), (400, Some("email")));
    let (s, v) = contact(
        &app,
        &token,
        &acme,
        json!({"full_name": "А", "phone": "12"}),
    )
    .await;
    assert_eq!((s, v["field"].as_str()), (400, Some("phone")));
    let (s, v) = contact(
        &app,
        &token,
        &acme,
        json!({"full_name": "А", "position_id": "00000000-0000-0000-0000-000000000001"}),
    )
    .await;
    assert_eq!((s, v["field"].as_str()), (400, Some("position_id")));
    app.close().await;
}

#[tokio::test]
async fn a_position_of_another_organization_is_rejected() {
    let app = setup().await;
    let a = app.token(TenantId::new(), &[Role::Recruiter]);
    let b = app.token(TenantId::new(), &[Role::Recruiter]);
    let foreign = position(&app, &a, "Директор").await;
    let own_client = client(&app, &b, "Globex").await;
    let (s, v) = contact(
        &app,
        &b,
        &own_client,
        json!({"full_name": "А", "position_id": foreign}),
    )
    .await;
    assert_eq!((s, v["field"].as_str()), (400, Some("position_id")), "{v}");
    app.close().await;
}

#[tokio::test]
async fn a_contact_can_be_edited_and_deleted() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let acme = client(&app, &token, "Acme").await;
    let pos = position(&app, &token, "HR").await;
    let (_, created) = contact(
        &app,
        &token,
        &acme,
        json!({"full_name": "Мария", "phone": "8 900 123-45-67"}),
    )
    .await;
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["phone"], "+79001234567");

    let (s, v) = app
        .send(
            "PATCH",
            &format!("/contacts/{id}"),
            Some(&token),
            Some(json!({"position_id": pos, "email": "M@Acme.io"})),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["position"]["name"], "HR");
    assert_eq!(v["email"], "m@acme.io");
    assert_eq!(v["full_name"], "Мария");

    let (s, v) = app
        .send(
            "PATCH",
            &format!("/contacts/{id}"),
            Some(&token),
            Some(json!({"position_id": null, "phone": null})),
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(v["position"], Value::Null);
    assert_eq!(v["phone"], Value::Null);

    let (s, _) = app
        .send("DELETE", &format!("/contacts/{id}"), Some(&token), None)
        .await;
    assert_eq!(s, 204);
    let (_, card) = app
        .send("GET", &format!("/clients/{acme}"), Some(&token), None)
        .await;
    assert_eq!(card["contacts"].as_array().unwrap().len(), 0);
    let (s, _) = app
        .send("DELETE", &format!("/contacts/{id}"), Some(&token), None)
        .await;
    assert_eq!(s, 404);
    app.close().await;
}

#[tokio::test]
async fn another_organizations_contact_is_not_found() {
    let app = setup().await;
    let a = app.token(TenantId::new(), &[Role::Recruiter]);
    let b = app.token(TenantId::new(), &[Role::Recruiter]);
    let acme = client(&app, &a, "Acme").await;
    let (_, created) = contact(&app, &a, &acme, json!({"full_name": "Мария"})).await;
    let id = created["id"].as_str().unwrap();
    let (s, _) = app
        .send(
            "PATCH",
            &format!("/contacts/{id}"),
            Some(&b),
            Some(json!({"full_name": "Взлом"})),
        )
        .await;
    assert_eq!(s, 404);
    let (s, _) = app
        .send("DELETE", &format!("/contacts/{id}"), Some(&b), None)
        .await;
    assert_eq!(s, 404);
    app.close().await;
}
