//! Список организаций на гриде: сортировка и фильтры `GridQuery` вместе с прежними параметрами.
mod common;

use common::{Ctx, setup};
use serde_json::{Value, json};
use uuid::Uuid;

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

async fn seed(ctx: &Ctx, slug: &str, name: &str, status: &str) {
    sqlx::query("INSERT INTO tenants (id, slug, name, status) VALUES ($1, $2, $3, $4)")
        .bind(Uuid::new_v4())
        .bind(slug)
        .bind(name)
        .bind(status)
        .execute(&ctx.db.owner)
        .await
        .unwrap();
}

async fn get(ctx: &Ctx, token: &str, query: &str) -> (u16, Value) {
    ctx.send(
        "GET",
        &format!("/tenants?{query}"),
        Some(token),
        Value::Null,
    )
    .await
}

fn slugs(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["slug"].as_str().unwrap().to_string())
        .collect()
}

async fn fixture() -> (Ctx, String) {
    let ctx = setup().await;
    seed(&ctx, "acme", "Acme Inc", "active").await;
    seed(&ctx, "beta-co", "Beta Company", "suspended").await;
    seed(&ctx, "gamma", "Gamma", "active").await;
    let token = ctx.admin_token().await;
    (ctx, token)
}

#[tokio::test]
async fn sorting_by_name_descending() {
    let (ctx, token) = fixture().await;
    let (s, page) = get(&ctx, &token, "sort=name:desc").await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(slugs(&page), ["gamma", "beta-co", "acme"]);
    ctx.close().await;
}

#[tokio::test]
async fn a_status_filter_and_a_text_search_combine() {
    let (ctx, token) = fixture().await;
    let both = filter(json!({"logic": "and", "filters": [
        {"field": "status", "operator": "eq", "value": "active"},
        {"logic": "or", "filters": [
            {"field": "slug", "operator": "contains", "value": "AM"},
            {"field": "name", "operator": "contains", "value": "inc"}
        ]}
    ]}));
    let (s, page) = get(&ctx, &token, &format!("{both}&sort=slug:asc")).await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(slugs(&page), ["acme", "gamma"]);
    assert_eq!(page["total"], 2);
    ctx.close().await;
}

#[tokio::test]
async fn without_a_sort_organizations_come_by_slug() {
    let (ctx, token) = fixture().await;
    let f = filter(json!({"field": "name", "operator": "contains", "value": ""}));
    let (s, page) = get(&ctx, &token, &f).await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(slugs(&page), ["acme", "beta-co", "gamma"]);
    ctx.close().await;
}

#[tokio::test]
async fn unknown_fields_are_refused() {
    let (ctx, token) = fixture().await;
    let (s, _) = get(&ctx, &token, "sort=secret:asc").await;
    assert_eq!(s, 400);
    ctx.close().await;
}

#[tokio::test]
async fn a_page_is_cut_and_counted_and_old_parameters_still_work() {
    let (ctx, token) = fixture().await;
    let (_, page) = get(&ctx, &token, "sort=slug:asc&page=2&page_size=2").await;
    assert_eq!(slugs(&page), ["gamma"]);
    assert_eq!(page["total"], 3);
    let (s, old) = get(&ctx, &token, "status=suspended").await;
    assert_eq!(s, 200);
    assert_eq!(slugs(&old), ["beta-co"]);
    ctx.close().await;
}

#[tokio::test]
async fn only_the_platform_admin_may_list() {
    let (ctx, _) = fixture().await;
    let (s, _) = ctx
        .send("GET", "/tenants?sort=slug:asc", None, Value::Null)
        .await;
    assert_eq!(s, 401);
    ctx.close().await;
}
