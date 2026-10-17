//! Список заказчиков на гриде: `GET /clients/grid`.
mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};

fn encode(raw: &str) -> String {
    raw.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn filter(value: Value) -> String {
    format!("filter={}", encode(&value.to_string()))
}

async fn create(app: &App, token: &str, name: &str) -> Value {
    let (s, v) = app
        .send("POST", "/clients", Some(token), Some(json!({"name": name})))
        .await;
    assert_eq!(s, 201, "{v}");
    v
}

async fn get(app: &App, token: &str, query: &str) -> (u16, Value) {
    app.send("GET", &format!("/clients/grid?{query}"), Some(token), None)
        .await
}

fn names(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect()
}

/// Три заказчика: «Альфа» (2 контакта), «Бета» (архивный), «Гамма».
async fn fixture() -> (App, String) {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let alpha = create(&app, &token, "Альфа").await;
    let beta = create(&app, &token, "Бета").await;
    create(&app, &token, "Гамма").await;
    for name in ["Анна", "Олег"] {
        let (s, v) = app
            .send(
                "POST",
                &format!("/clients/{}/contacts", alpha["id"].as_str().unwrap()),
                Some(&token),
                Some(json!({"full_name": name})),
            )
            .await;
        assert_eq!(s, 201, "{v}");
    }
    let (s, v) = app
        .send(
            "PATCH",
            &format!("/clients/{}", beta["id"].as_str().unwrap()),
            Some(&token),
            Some(json!({"archived": true})),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    (app, token)
}

#[tokio::test]
async fn by_default_only_active_clients_come_by_name() {
    let (app, token) = fixture().await;
    let (s, page) = get(&app, &token, "").await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(names(&page), ["Альфа", "Гамма"]);
    assert_eq!(page["total"], 2);
    assert_eq!(page["page"], 1);
    assert_eq!(page["items"][0]["contacts_count"], 2);
}

#[tokio::test]
async fn the_value_all_includes_archived_clients_after_the_active_ones() {
    let (app, token) = fixture().await;
    let all = filter(json!({"field": "archived", "operator": "eq", "value": "all"}));
    let (s, page) = get(&app, &token, &all).await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(names(&page), ["Альфа", "Гамма", "Бета"]);
    assert_eq!(page["total"], 3);
}

#[tokio::test]
async fn all_can_be_combined_with_other_conditions() {
    let (app, token) = fixture().await;
    let f = filter(json!({"logic": "and", "filters": [
        {"field": "archived", "operator": "eq", "value": "all"},
        {"field": "name", "operator": "contains", "value": "бет"}
    ]}));
    let (s, page) = get(&app, &token, &f).await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(names(&page), ["Бета"]);
}

#[tokio::test]
async fn sorting_by_the_number_of_contacts_works() {
    let (app, token) = fixture().await;
    let (s, page) = get(&app, &token, "sort=contacts_count:desc").await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(names(&page)[0], "Альфа");
}

#[tokio::test]
async fn the_archive_flag_and_the_name_filter_work() {
    let (app, token) = fixture().await;
    let archived = filter(json!({"field": "archived", "operator": "eq", "value": true}));
    let (s, page) = get(&app, &token, &archived).await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(names(&page), ["Бета"]);

    let by_name = filter(json!({"field": "name", "operator": "contains", "value": "гам"}));
    let (_, page) = get(&app, &token, &by_name).await;
    assert_eq!(names(&page), ["Гамма"]);
}

#[tokio::test]
async fn pages_are_cut_by_the_requested_size() {
    let (app, token) = fixture().await;
    let (_, page) = get(&app, &token, "page=2&page_size=1").await;
    assert_eq!(names(&page), ["Гамма"]);
    assert_eq!(page["total"], 2);
    assert_eq!(page["page_size"], 1);
}

#[tokio::test]
async fn unknown_fields_are_refused() {
    let (app, token) = fixture().await;
    let (s, _) = get(&app, &token, "sort=tenant_id:asc").await;
    assert_eq!(s, 400);
    let f = filter(json!({"field": "name_key", "operator": "eq", "value": "x"}));
    let (s, _) = get(&app, &token, &f).await;
    assert_eq!(s, 400);
}

#[tokio::test]
async fn another_organization_sees_nothing_and_the_old_list_still_works() {
    let (app, token) = fixture().await;
    let other = app.token(TenantId::new(), &[Role::Owner]);
    let (s, page) = get(&app, &other, "").await;
    assert_eq!(s, 200);
    assert_eq!(page["total"], 0);
    // прежний список без архивных остаётся массивом
    let (s, list) = app.send("GET", "/clients", Some(&token), None).await;
    assert_eq!(s, 200);
    assert_eq!(list.as_array().unwrap().len(), 2);
}
