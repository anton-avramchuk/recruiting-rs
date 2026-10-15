mod common;

use common::{App, SERVICE_TOKEN, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};

async fn create(app: &App, token: &str, body: Value) -> Value {
    let (s, p) = app.send("POST", "/persons", Some(token), Some(body)).await;
    assert_eq!(s, 201, "{p}");
    p
}

async fn feed(app: &App, tenant: TenantId, after: i64) -> Value {
    let (s, v) = app
        .send(
            "GET",
            &format!(
                "/internal/tenants/{}/person-changes?after={after}",
                tenant.0
            ),
            Some(SERVICE_TOKEN),
            None,
        )
        .await;
    assert_eq!(s, 200, "{v}");
    v
}

#[tokio::test]
async fn feed_returns_latest_state_of_changed_people_after_position() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let a = create(
        &app,
        &token,
        json!({"full_name": "Анна", "city": "Казань", "skills": ["Go"]}),
    )
    .await;
    let first = feed(&app, tenant, 0).await;
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    let pos = first["next"].as_i64().unwrap();
    assert_eq!(first["items"][0]["snapshot"]["city"], "Казань");

    create(
        &app,
        &token,
        json!({"full_name": "Борис", "email": "b@x.ru"}),
    )
    .await;
    let (s, _) = app
        .send(
            "PATCH",
            &format!("/persons/{}", a["id"].as_str().unwrap()),
            Some(&token),
            Some(json!({"city": "Москва"})),
        )
        .await;
    assert_eq!(s, 200);

    let page = feed(&app, tenant, pos).await;
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{page}");
    assert!(page["next"].as_i64().unwrap() > pos);
    let anna = items.iter().find(|i| i["person_id"] == a["id"]).unwrap();
    assert_eq!(anna["snapshot"]["city"], "Москва");
    assert_eq!(anna["deleted"], false);
    // позиции возрастают
    assert!(items[0]["seq"].as_i64() < items[1]["seq"].as_i64());
    app.close().await;
}

#[tokio::test]
async fn repeated_edits_leave_one_entry_with_the_last_state() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let p = create(&app, &token, json!({"full_name": "Анна"})).await;
    let uri = format!("/persons/{}", p["id"].as_str().unwrap());
    for city in ["A", "B", "C", "D", "E"] {
        app.send("PATCH", &uri, Some(&token), Some(json!({"city": city})))
            .await;
    }
    let page = feed(&app, tenant, 0).await;
    assert_eq!(page["items"].as_array().unwrap().len(), 1, "{page}");
    assert_eq!(page["items"][0]["snapshot"]["city"], "E");
    app.close().await;
}

#[tokio::test]
async fn employments_change_experience_in_the_feed() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let p = create(&app, &token, json!({"full_name": "Анна"})).await;
    let (s, _) = app
        .send(
            "PUT",
            &format!("/persons/{}/employments", p["id"].as_str().unwrap()),
            Some(&token),
            Some(json!([{"company": "X", "position": "Dev", "start": "2020-01-01"}])),
        )
        .await;
    assert_eq!(s, 200);
    let page = feed(&app, tenant, 0).await;
    assert_eq!(page["items"][0]["snapshot"]["experience_years"], 6);
    app.close().await;
}

#[tokio::test]
async fn deletion_is_published_as_a_tombstone() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Owner]);
    let p = create(&app, &token, json!({"full_name": "Анна"})).await;
    let (s, _) = app
        .send(
            "DELETE",
            &format!("/persons/{}", p["id"].as_str().unwrap()),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(s, 204);
    let page = feed(&app, tenant, 0).await;
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{page}");
    assert_eq!(items[0]["person_id"], p["id"]);
    assert_eq!(items[0]["deleted"], true);
    assert_eq!(items[0]["snapshot"], Value::Null);
    app.close().await;
}

#[tokio::test]
async fn feed_is_per_tenant_and_service_only() {
    let app = setup().await;
    let (a, b) = (TenantId::new(), TenantId::new());
    let ta = app.token(a, &[Role::Recruiter]);
    create(&app, &ta, json!({"full_name": "Анна"})).await;
    assert_eq!(feed(&app, b, 0).await["items"].as_array().unwrap().len(), 0);

    let uri = format!("/internal/tenants/{}/person-changes", a.0);
    let (s, _) = app.send("GET", &uri, None, None).await;
    assert_eq!(s, 401);
    let (s, _) = app.send("GET", &uri, Some(&ta), None).await;
    assert_eq!(s, 401, "user token is not a service token");
    app.close().await;
}

#[tokio::test]
async fn feed_is_paged_by_limit() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    for n in 0..3 {
        create(&app, &token, json!({"full_name": format!("Человек {n}")})).await;
    }
    let (s, page) = app
        .send(
            "GET",
            &format!(
                "/internal/tenants/{}/person-changes?after=0&limit=2",
                tenant.0
            ),
            Some(SERVICE_TOKEN),
            None,
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    let rest = feed(&app, tenant, page["next"].as_i64().unwrap()).await;
    assert_eq!(rest["items"].as_array().unwrap().len(), 1);
    app.close().await;
}
