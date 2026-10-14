mod common;

use common::{Ctx, SERVICE_TOKEN, setup};
use platform_auth::{Audience, Role};
use serde_json::{Value, json};

async fn create(ctx: &Ctx, token: &str, slug: &str) -> Value {
    let (s, t) = ctx
        .send(
            "POST",
            "/tenants",
            Some(token),
            json!({"slug": slug, "name": "N", "owner": {"email": "o@x.io", "password": "long-enough-1"}}),
        )
        .await;
    assert_eq!(s, 201, "{t}");
    t
}

#[tokio::test]
async fn suspend_and_activate_follow_the_lifecycle() {
    let ctx = setup().await;
    let token = ctx.admin_token().await;
    let t = create(&ctx, &token, "acme").await;
    let id = t["id"].as_str().unwrap();

    let (s, suspended) = ctx
        .send(
            "POST",
            &format!("/tenants/{id}/suspend"),
            Some(&token),
            Value::Null,
        )
        .await;
    assert_eq!((s, suspended["status"].as_str()), (200, Some("suspended")));
    // повторная приостановка — конфликт состояния
    let (s, _) = ctx
        .send(
            "POST",
            &format!("/tenants/{id}/suspend"),
            Some(&token),
            Value::Null,
        )
        .await;
    assert_eq!(s, 409);

    let (s, active) = ctx
        .send(
            "POST",
            &format!("/tenants/{id}/activate"),
            Some(&token),
            Value::Null,
        )
        .await;
    assert_eq!((s, active["status"].as_str()), (200, Some("active")));
    let (s, _) = ctx
        .send(
            "POST",
            &format!("/tenants/{id}/activate"),
            Some(&token),
            Value::Null,
        )
        .await;
    assert_eq!(s, 409);
    ctx.close().await;
}

#[tokio::test]
async fn unknown_tenant_is_not_found_and_auth_is_required() {
    let ctx = setup().await;
    let token = ctx.admin_token().await;
    let missing = uuid::Uuid::new_v4();
    for action in ["suspend", "activate"] {
        let uri = format!("/tenants/{missing}/{action}");
        assert_eq!(
            ctx.send("POST", &uri, Some(&token), Value::Null).await.0,
            404
        );
        assert_eq!(ctx.send("POST", &uri, None, Value::Null).await.0, 401);
        let tenant_tok = ctx.forge(Audience::Tenant, vec![Role::Owner]);
        assert_eq!(
            ctx.send("POST", &uri, Some(&tenant_tok), Value::Null)
                .await
                .0,
            403
        );
    }
    ctx.close().await;
}

#[tokio::test]
async fn internal_directory_reflects_current_status() {
    let ctx = setup().await;
    let token = ctx.admin_token().await;
    let t = create(&ctx, &token, "acme").await;
    let id = t["id"].as_str().unwrap();

    let (s, by_slug) = ctx
        .send(
            "GET",
            "/internal/tenants/by-slug/acme",
            Some(SERVICE_TOKEN),
            Value::Null,
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(
        (by_slug["id"].as_str(), by_slug["status"].as_str()),
        (Some(id), Some("active"))
    );

    ctx.send(
        "POST",
        &format!("/tenants/{id}/suspend"),
        Some(&token),
        Value::Null,
    )
    .await;
    let (_, by_id) = ctx
        .send(
            "GET",
            &format!("/internal/tenants/{id}"),
            Some(SERVICE_TOKEN),
            Value::Null,
        )
        .await;
    assert_eq!(by_id["status"], "suspended");

    let missing = ctx
        .send(
            "GET",
            "/internal/tenants/by-slug/ghost",
            Some(SERVICE_TOKEN),
            Value::Null,
        )
        .await;
    assert_eq!(missing.0, 404);
    ctx.close().await;
}

#[tokio::test]
async fn internal_directory_accepts_only_the_service_token() {
    let ctx = setup().await;
    let token = ctx.admin_token().await;
    create(&ctx, &token, "acme").await;
    let uri = "/internal/tenants/by-slug/acme";
    assert_eq!(ctx.send("GET", uri, None, Value::Null).await.0, 401);
    assert_eq!(
        ctx.send("GET", uri, Some("wrong"), Value::Null).await.0,
        401
    );
    // JWT администратора — не сервисный токен
    assert_eq!(ctx.send("GET", uri, Some(&token), Value::Null).await.0, 401);
    ctx.close().await;
}
