mod common;

use std::sync::atomic::Ordering;

use common::setup;
use platform_auth::{Audience, Role};
use serde_json::{Value, json};

fn body(slug: &str) -> Value {
    json!({
        "slug": slug,
        "name": "Acme Inc",
        "owner": {"email": "Boss@Acme.io", "password": "long-enough-1", "display_name": "Boss"}
    })
}

async fn status_in_db(ctx: &common::Ctx, slug: &str) -> Option<String> {
    sqlx::query_scalar("SELECT status FROM tenants WHERE slug = $1")
        .bind(slug)
        .fetch_optional(&ctx.db.owner)
        .await
        .unwrap()
}

#[tokio::test]
async fn creates_active_tenant_and_provisions_the_owner() {
    let ctx = setup().await;
    let token = ctx.admin_token().await;

    let (s, tenant) = ctx
        .send("POST", "/tenants", Some(&token), body("acme"))
        .await;
    assert_eq!(s, 201, "{tenant}");
    assert_eq!(tenant["slug"], "acme");
    assert_eq!(tenant["status"], "active");
    assert_eq!(status_in_db(&ctx, "acme").await.as_deref(), Some("active"));

    let calls = ctx.prov.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0.to_string(), tenant["id"].as_str().unwrap());
    assert_eq!(calls[0].1.email, "boss@acme.io");
    // владелец создаётся, пока тенант ещё в статусе provisioning
    assert_eq!(*ctx.prov.status_at_call.lock().unwrap(), ["provisioning"]);
    ctx.close().await;
}

#[tokio::test]
async fn duplicate_slug_conflicts_without_touching_identity() {
    let ctx = setup().await;
    let token = ctx.admin_token().await;
    assert_eq!(
        ctx.send("POST", "/tenants", Some(&token), body("acme"))
            .await
            .0,
        201
    );

    let (s, _) = ctx
        .send("POST", "/tenants", Some(&token), body("acme"))
        .await;
    assert_eq!(s, 409);
    assert_eq!(ctx.prov.calls.lock().unwrap().len(), 1);
    ctx.close().await;
}

#[tokio::test]
async fn failed_provisioning_leaves_no_tenant_and_frees_the_slug() {
    let ctx = setup().await;
    let token = ctx.admin_token().await;
    ctx.prov.fail.store(true, Ordering::SeqCst);

    let (s, _) = ctx
        .send("POST", "/tenants", Some(&token), body("acme"))
        .await;
    assert_eq!(s, 502);
    assert_eq!(status_in_db(&ctx, "acme").await, None);

    // после восстановления identity тот же slug создаётся
    ctx.prov.fail.store(false, Ordering::SeqCst);
    let (s, _) = ctx
        .send("POST", "/tenants", Some(&token), body("acme"))
        .await;
    assert_eq!(s, 201);
    ctx.close().await;
}

#[tokio::test]
async fn invalid_input_is_rejected_before_any_side_effect() {
    let ctx = setup().await;
    let token = ctx.admin_token().await;
    for slug in [
        "",
        "a",
        "Acme",
        "has space",
        "-lead",
        "trail-",
        "double--dash",
        "admin",
        "x".repeat(41).as_str(),
    ] {
        let (s, _) = ctx.send("POST", "/tenants", Some(&token), body(slug)).await;
        assert_eq!(s, 400, "slug {slug:?}");
    }
    let mut weak = body("ok-slug");
    weak["owner"]["password"] = json!("x");
    assert_eq!(
        ctx.send("POST", "/tenants", Some(&token), weak).await.0,
        400
    );
    let mut bad_email = body("ok-slug");
    bad_email["owner"]["email"] = json!("nope");
    assert_eq!(
        ctx.send("POST", "/tenants", Some(&token), bad_email)
            .await
            .0,
        400
    );

    assert!(ctx.prov.calls.lock().unwrap().is_empty());
    assert_eq!(status_in_db(&ctx, "ok-slug").await, None);
    ctx.close().await;
}

#[tokio::test]
async fn only_platform_admins_may_create_tenants() {
    let ctx = setup().await;
    assert_eq!(
        ctx.send("POST", "/tenants", None, body("acme")).await.0,
        401
    );
    let tenant_tok = ctx.forge(Audience::Tenant, vec![Role::Owner]);
    assert_eq!(
        ctx.send("POST", "/tenants", Some(&tenant_tok), body("acme"))
            .await
            .0,
        403
    );
    assert_eq!(status_in_db(&ctx, "acme").await, None);
    ctx.close().await;
}
