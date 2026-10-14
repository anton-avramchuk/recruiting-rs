mod common;

use common::{SERVICE_TOKEN, setup};
use identity::application::tenants::TenantStatus;
use serde_json::json;

fn owner_body() -> serde_json::Value {
    json!({"email": "Boss@Acme.io", "password": "long-enough-1", "display_name": "Boss"})
}

#[tokio::test]
async fn internal_endpoint_requires_service_token() {
    let ctx = setup().await;
    let tid = ctx.dir.add("acme", TenantStatus::Active);
    let uri = format!("/internal/tenants/{tid}/owner");
    assert_eq!(ctx.send("POST", &uri, None, owner_body()).await.0, 401);
    assert_eq!(
        ctx.send("POST", &uri, Some("wrong"), owner_body()).await.0,
        401
    );
    ctx.close().await;
}

#[tokio::test]
async fn creates_owner_who_can_log_in() {
    let ctx = setup().await;
    let tid = ctx.dir.add("acme", TenantStatus::Active);
    let uri = format!("/internal/tenants/{tid}/owner");

    let (s, body) = ctx
        .send("POST", &uri, Some(SERVICE_TOKEN), owner_body())
        .await;
    assert_eq!(s, 201, "{body}");
    assert_eq!(body["email"], "boss@acme.io");
    assert_eq!(body["roles"], json!(["owner"]));

    let (s, _) = ctx
        .post(
            "/auth/login",
            json!({"tenant":"acme","email":"boss@acme.io","password":"long-enough-1"}),
        )
        .await;
    assert_eq!(s, 200);
    ctx.close().await;
}

#[tokio::test]
async fn repeated_call_is_idempotent() {
    let ctx = setup().await;
    let tid = ctx.dir.add("acme", TenantStatus::Active);
    let uri = format!("/internal/tenants/{tid}/owner");

    let (s1, first) = ctx
        .send("POST", &uri, Some(SERVICE_TOKEN), owner_body())
        .await;
    let (s2, second) = ctx
        .send("POST", &uri, Some(SERVICE_TOKEN), owner_body())
        .await;
    assert_eq!((s1, s2), (201, 200));
    assert_eq!(first["id"], second["id"]);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&ctx.db.owner)
        .await
        .unwrap();
    assert_eq!(n, 1);
    ctx.close().await;
}

#[tokio::test]
async fn weak_password_is_rejected() {
    let ctx = setup().await;
    let tid = ctx.dir.add("acme", TenantStatus::Active);
    let mut body = owner_body();
    body["password"] = json!("x");
    let (s, _) = ctx
        .send(
            "POST",
            &format!("/internal/tenants/{tid}/owner"),
            Some(SERVICE_TOKEN),
            body,
        )
        .await;
    assert_eq!(s, 400);
    ctx.close().await;
}
