//! Список людей на гриде: сортировка и фильтры `GridQuery`, изоляция организаций, прежние параметры.
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

async fn add(app: &App, token: &str, body: Value) {
    let mut body = body;
    body["confirm_duplicate"] = json!(true);
    let (s, p) = app.send("POST", "/persons", Some(token), Some(body)).await;
    assert_eq!(s, 201, "{p}");
}

async fn get(app: &App, token: &str, query: &str) -> (u16, Value) {
    let (s, body) = app
        .send("GET", &format!("/persons?{query}"), Some(token), None)
        .await;
    (s, body)
}

fn names(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["full_name"].as_str().unwrap().to_string())
        .collect()
}

async fn fixture() -> (App, String) {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    for body in [
        json!({"full_name": "Анна Смирнова", "email": "anna@mail.ru", "source": "hh.ru",
               "expected_salary": 300000, "skills": ["Rust", "SQL"], "declared_experience_years": 6}),
        json!({"full_name": "Борис Иванов", "email": "boris@example.com", "source": "Реферал",
               "expected_salary": 200000, "skills": ["Go"], "declared_experience_years": 2}),
        json!({"full_name": "Виктор Петров", "source": "hh.ru", "skills": ["Docker"],
               "declared_experience_years": 4}),
    ] {
        add(&app, &token, body).await;
    }
    (app, token)
}

#[tokio::test]
async fn sorting_by_a_field_orders_the_page() {
    let (app, token) = fixture().await;
    let (s, page) = get(&app, &token, "sort=full_name:desc").await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(
        names(&page),
        ["Виктор Петров", "Борис Иванов", "Анна Смирнова"]
    );
    let (_, asc) = get(&app, &token, "sort=expected_salary:asc").await;
    assert_eq!(names(&asc)[0], "Борис Иванов");
}

#[tokio::test]
async fn a_filter_keeps_matching_people_and_counts_them() {
    let (app, token) = fixture().await;
    let f = filter(json!({"field": "source", "operator": "eq", "value": "hh.ru"}));
    let (s, page) = get(&app, &token, &format!("{f}&sort=full_name:asc")).await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(names(&page), ["Анна Смирнова", "Виктор Петров"]);
    assert_eq!(page["total"], 2);
}

#[tokio::test]
async fn skills_are_searched_as_text() {
    let (app, token) = fixture().await;
    let f = filter(json!({"field": "skills", "operator": "contains", "value": "rust"}));
    let (s, page) = get(&app, &token, &f).await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(names(&page), ["Анна Смирнова"]);
}

#[tokio::test]
async fn a_group_of_conditions_works_for_the_search_box() {
    let (app, token) = fixture().await;
    let f = filter(json!({"logic": "or", "filters": [
        {"field": "full_name", "operator": "contains", "value": "борис"},
        {"field": "email", "operator": "contains", "value": "anna"}
    ]}));
    let (s, page) = get(&app, &token, &format!("{f}&sort=full_name:asc")).await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(names(&page), ["Анна Смирнова", "Борис Иванов"]);
}

#[tokio::test]
async fn an_unknown_field_is_rejected_and_nothing_is_returned() {
    let (app, token) = fixture().await;
    let (s, body) = get(&app, &token, "sort=password:asc").await;
    assert_eq!(s, 400, "{body}");
    let f = filter(json!({"field": "normalized_email", "operator": "eq", "value": "x"}));
    let (s, _) = get(&app, &token, &f).await;
    assert_eq!(s, 400);
}

#[tokio::test]
async fn another_organization_sees_none_of_the_people() {
    let (app, _) = fixture().await;
    let other = app.token(TenantId::new(), &[Role::Owner]);
    let f = filter(json!({"field": "full_name", "operator": "contains", "value": ""}));
    let (s, page) = get(&app, &other, &f).await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(page["total"], 0);
}

#[tokio::test]
async fn the_old_parameters_still_work_and_keep_the_default_order() {
    let (app, token) = fixture().await;
    let (s, page) = get(&app, &token, "page=1&page_size=2").await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(page["total"], 3);
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    assert_eq!(page["page_size"], 2);
    // новые первыми, как раньше
    assert_eq!(names(&page)[0], "Виктор Петров");
}

#[tokio::test]
async fn the_grid_page_has_the_same_shape_as_before() {
    let (app, token) = fixture().await;
    let (_, page) = get(&app, &token, "sort=full_name:asc&page=2&page_size=2").await;
    assert_eq!(page["page"], 2);
    assert_eq!(page["page_size"], 2);
    assert_eq!(page["total"], 3);
    assert_eq!(names(&page), ["Виктор Петров"]);
    assert!(page["items"][0]["skills"].is_array());
    assert!(page["items"][0]["created_at"].is_string());
}
