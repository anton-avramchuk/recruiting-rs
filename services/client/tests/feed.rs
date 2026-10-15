mod common;

use common::{App, SERVICE_TOKEN, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};

async fn feed(app: &App, tenant: TenantId, after: i64) -> Value {
    let (s, v) = app
        .send(
            "GET",
            &format!(
                "/internal/tenants/{}/client-changes?after={after}",
                tenant.0
            ),
            Some(SERVICE_TOKEN),
            None,
        )
        .await;
    assert_eq!(s, 200, "{v}");
    v
}

fn find<'a>(page: &'a Value, id: &Value) -> &'a Value {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| &i["entity_id"] == id)
        .unwrap_or_else(|| panic!("no change for {id} in {page}"))
}

#[tokio::test]
async fn the_feed_carries_clients_and_contacts_with_their_latest_state() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let (_, client) = app
        .send(
            "POST",
            "/clients",
            Some(&token),
            Some(json!({"name": "Acme"})),
        )
        .await;
    let (_, pos) = app
        .send(
            "POST",
            "/positions",
            Some(&token),
            Some(json!({"name": "HR"})),
        )
        .await;
    let cid = client["id"].as_str().unwrap();
    let (_, contact) = app
        .send(
            "POST",
            &format!("/clients/{cid}/contacts"),
            Some(&token),
            Some(json!({"full_name": "Мария", "position_id": pos["id"]})),
        )
        .await;

    let page = feed(&app, tenant, 0).await;
    let c = find(&page, &client["id"]);
    assert_eq!(c["entity"], "client");
    assert_eq!(c["client"]["name"], "Acme");
    assert_eq!(c["client"]["archived"], false);
    let k = find(&page, &contact["id"]);
    assert_eq!(k["entity"], "contact");
    assert_eq!(k["contact"]["full_name"], "Мария");
    assert_eq!(k["contact"]["position"], "HR");
    assert_eq!(k["contact"]["client_id"], client["id"]);
    app.close().await;
}

#[tokio::test]
async fn reading_from_a_position_returns_only_later_changes() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let (_, a) = app
        .send(
            "POST",
            "/clients",
            Some(&token),
            Some(json!({"name": "Acme"})),
        )
        .await;
    let first = feed(&app, tenant, 0).await;
    let pos = first["next"].as_i64().unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 1);

    app.send(
        "POST",
        "/clients",
        Some(&token),
        Some(json!({"name": "Globex"})),
    )
    .await;
    app.send(
        "PATCH",
        &format!("/clients/{}", a["id"].as_str().unwrap()),
        Some(&token),
        Some(json!({"archived": true})),
    )
    .await;

    let page = feed(&app, tenant, pos).await;
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{page}");
    assert!(page["next"].as_i64().unwrap() > pos);
    assert!(items[0]["seq"].as_i64() < items[1]["seq"].as_i64());
    // одна запись на сущность: прежнее состояние Acme вытеснено
    assert_eq!(find(&page, &a["id"])["client"]["archived"], true);
    // с конца ничего нет, позиция не двигается
    let end = feed(&app, tenant, page["next"].as_i64().unwrap()).await;
    assert_eq!(end["items"].as_array().unwrap().len(), 0);
    assert_eq!(end["next"], page["next"]);
    app.close().await;
}

#[tokio::test]
async fn a_deleted_contact_leaves_a_tombstone() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let (_, client) = app
        .send(
            "POST",
            "/clients",
            Some(&token),
            Some(json!({"name": "Acme"})),
        )
        .await;
    let (_, contact) = app
        .send(
            "POST",
            &format!("/clients/{}/contacts", client["id"].as_str().unwrap()),
            Some(&token),
            Some(json!({"full_name": "Мария"})),
        )
        .await;
    let pos = feed(&app, tenant, 0).await["next"].as_i64().unwrap();
    app.send(
        "DELETE",
        &format!("/contacts/{}", contact["id"].as_str().unwrap()),
        Some(&token),
        None,
    )
    .await;

    let page = feed(&app, tenant, pos).await;
    let k = find(&page, &contact["id"]);
    assert_eq!(k["deleted"], true);
    assert!(k.get("contact").is_none());
    app.close().await;
}

#[tokio::test]
async fn renaming_a_position_refreshes_all_its_contacts_in_the_feed() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let (_, client) = app
        .send(
            "POST",
            "/clients",
            Some(&token),
            Some(json!({"name": "Acme"})),
        )
        .await;
    let (_, pos) = app
        .send(
            "POST",
            "/positions",
            Some(&token),
            Some(json!({"name": "Директор по персоналу"})),
        )
        .await;
    let cid = client["id"].as_str().unwrap();
    let mut ids = Vec::new();
    for name in ["Мария", "Иван"] {
        let (_, c) = app
            .send(
                "POST",
                &format!("/clients/{cid}/contacts"),
                Some(&token),
                Some(json!({"full_name": name, "position_id": pos["id"]})),
            )
            .await;
        ids.push(c["id"].clone());
    }
    let position = feed(&app, tenant, 0).await["next"].as_i64().unwrap();

    app.send(
        "PATCH",
        &format!("/positions/{}", pos["id"].as_str().unwrap()),
        Some(&token),
        Some(json!({"name": "HR-директор"})),
    )
    .await;

    let page = feed(&app, tenant, position).await;
    assert_eq!(page["items"].as_array().unwrap().len(), 2, "{page}");
    for id in &ids {
        assert_eq!(find(&page, id)["contact"]["position"], "HR-директор");
    }
    app.close().await;
}

#[tokio::test]
async fn the_feed_is_per_organization_and_needs_the_service_token() {
    let app = setup().await;
    let (a, b) = (TenantId::new(), TenantId::new());
    let token = app.token(a, &[Role::Recruiter]);
    app.send(
        "POST",
        "/clients",
        Some(&token),
        Some(json!({"name": "Acme"})),
    )
    .await;

    assert_eq!(feed(&app, a, 0).await["items"].as_array().unwrap().len(), 1);
    assert_eq!(feed(&app, b, 0).await["items"].as_array().unwrap().len(), 0);

    let uri = format!("/internal/tenants/{}/client-changes", a.0);
    let (s, _) = app.send("GET", &uri, Some(&token), None).await;
    assert_eq!(s, 401, "a user token is not a service token");
    let (s, _) = app.send("GET", &uri, None, None).await;
    assert_eq!(s, 401);
    let (s, _) = app
        .send("GET", &format!("{uri}?after=-1"), Some(SERVICE_TOKEN), None)
        .await;
    assert_eq!(s, 400);
    app.close().await;
}
