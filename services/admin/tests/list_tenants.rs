mod common;

use common::{Ctx, setup};
use platform_auth::{Audience, Role};
use serde_json::{Value, json};
use uuid::Uuid;

async fn seed(ctx: &Ctx, slug: &str, name: &str, status: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, slug, name, status) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(slug)
        .bind(name)
        .bind(status)
        .execute(&ctx.db.owner)
        .await
        .unwrap();
    id
}

async fn list(ctx: &Ctx, token: &str, query: &str) -> Value {
    let (s, body) = ctx
        .send("GET", &format!("/tenants{query}"), Some(token), Value::Null)
        .await;
    assert_eq!(s, 200, "{body}");
    body
}

fn slugs(page: &Value) -> Vec<&str> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["slug"].as_str().unwrap())
        .collect()
}

async fn fixture() -> (Ctx, String) {
    let ctx = setup().await;
    seed(&ctx, "acme", "Acme Inc", "active").await;
    seed(&ctx, "beta-co", "Beta Company", "suspended").await;
    seed(&ctx, "gamma", "Gamma 100%", "active").await;
    seed(&ctx, "delta-acme", "Delta", "suspended").await;
    let token = ctx.admin_token().await;
    (ctx, token)
}

#[tokio::test]
async fn lists_all_tenants_ordered_by_slug() {
    let (ctx, token) = fixture().await;
    let page = list(&ctx, &token, "").await;
    assert_eq!(slugs(&page), ["acme", "beta-co", "delta-acme", "gamma"]);
    assert_eq!(page["total"], 4);
    assert_eq!(
        (page["page"].as_i64(), page["page_size"].as_i64()),
        (Some(1), Some(20))
    );
    ctx.close().await;
}

#[tokio::test]
async fn filters_by_status() {
    let (ctx, token) = fixture().await;
    let page = list(&ctx, &token, "?status=suspended").await;
    assert_eq!(slugs(&page), ["beta-co", "delta-acme"]);
    assert_eq!(page["total"], 2);
    let (s, _) = ctx
        .send("GET", "/tenants?status=bogus", Some(&token), Value::Null)
        .await;
    assert_eq!(s, 400);
    ctx.close().await;
}

#[tokio::test]
async fn searches_by_name_or_slug_case_insensitively() {
    let (ctx, token) = fixture().await;
    assert_eq!(
        slugs(&list(&ctx, &token, "?q=ACME").await),
        ["acme", "delta-acme"]
    );
    assert_eq!(slugs(&list(&ctx, &token, "?q=company").await), ["beta-co"]);
    // спецсимволы LIKE обрабатываются буквально
    assert_eq!(slugs(&list(&ctx, &token, "?q=100%25").await), ["gamma"]);
    // "%" не работает как подстановка: находится только имя, где он есть буквально
    assert_eq!(slugs(&list(&ctx, &token, "?q=%25").await), ["gamma"]);
    assert!(slugs(&list(&ctx, &token, "?q=_").await).is_empty());
    ctx.close().await;
}

#[tokio::test]
async fn paginates_and_reports_total() {
    let (ctx, token) = fixture().await;
    let p1 = list(&ctx, &token, "?page=1&page_size=3").await;
    assert_eq!(slugs(&p1), ["acme", "beta-co", "delta-acme"]);
    assert_eq!(p1["total"], 4);
    let p2 = list(&ctx, &token, "?page=2&page_size=3").await;
    assert_eq!(slugs(&p2), ["gamma"]);
    assert!(slugs(&list(&ctx, &token, "?page=9&page_size=3").await).is_empty());
    // страница фильтруется вместе с поиском
    let filtered = list(&ctx, &token, "?status=active&q=a&page_size=1").await;
    assert_eq!(
        (slugs(&filtered), filtered["total"].as_i64()),
        (vec!["acme"], Some(2))
    );
    ctx.close().await;
}

#[tokio::test]
async fn page_size_is_clamped_and_bad_paging_rejected() {
    let (ctx, token) = fixture().await;
    let page = list(&ctx, &token, "?page_size=1000").await;
    assert_eq!(page["page_size"], 100);
    for bad in ["?page=0", "?page_size=0", "?page=-1"] {
        let (s, _) = ctx
            .send("GET", &format!("/tenants{bad}"), Some(&token), Value::Null)
            .await;
        assert_eq!(s, 400, "{bad}");
    }
    ctx.close().await;
}

#[tokio::test]
async fn gets_one_tenant_and_requires_admin() {
    let (ctx, token) = fixture().await;
    let id = seed(&ctx, "solo", "Solo", "active").await;
    let (s, t) = ctx
        .send("GET", &format!("/tenants/{id}"), Some(&token), Value::Null)
        .await;
    assert_eq!((s, t["slug"].as_str()), (200, Some("solo")));
    let (s, _) = ctx
        .send(
            "GET",
            &format!("/tenants/{}", Uuid::new_v4()),
            Some(&token),
            Value::Null,
        )
        .await;
    assert_eq!(s, 404);

    assert_eq!(ctx.send("GET", "/tenants", None, json!(null)).await.0, 401);
    let tenant_tok = ctx.forge(Audience::Tenant, vec![Role::Owner]);
    assert_eq!(
        ctx.send("GET", "/tenants", Some(&tenant_tok), json!(null))
            .await
            .0,
        403
    );
    ctx.close().await;
}
