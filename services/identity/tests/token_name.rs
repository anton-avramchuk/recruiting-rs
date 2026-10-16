mod common;

use common::setup;
use identity::application::tenants::TenantStatus;
use serde_json::{Value, json};

async fn login(ctx: &common::Ctx, email: &str) -> Value {
    let (s, tokens) = ctx
        .post(
            "/auth/login",
            json!({"tenant": "acme", "email": email, "password": "s3cret-pass"}),
        )
        .await;
    assert_eq!(s, 200, "{tokens}");
    tokens
}

#[tokio::test]
async fn the_token_carries_the_display_name_and_falls_back_to_the_email() {
    let ctx = setup().await;
    let tid = ctx.dir.add("acme", TenantStatus::Active);
    let named = ctx
        .seed_user(tid, "maria@acme.io", "s3cret-pass", &["recruiter"])
        .await;
    ctx.seed_user(tid, "anon@acme.io", "s3cret-pass", &["recruiter"])
        .await;
    sqlx::query("UPDATE users SET display_name = 'Мария Петрова' WHERE id = $1")
        .bind(named)
        .execute(&ctx.db.owner)
        .await
        .unwrap();

    let tokens = login(&ctx, "maria@acme.io").await;
    let claims = ctx
        .verifier
        .verify(tokens["access_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(claims.name.as_deref(), Some("Мария Петрова"));

    let tokens = login(&ctx, "anon@acme.io").await;
    let claims = ctx
        .verifier
        .verify(tokens["access_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(claims.name.as_deref(), Some("anon@acme.io"));
    ctx.close().await;
}

#[tokio::test]
async fn a_refreshed_token_keeps_the_name() {
    let ctx = setup().await;
    let tid = ctx.dir.add("acme", TenantStatus::Active);
    let id = ctx
        .seed_user(tid, "maria@acme.io", "s3cret-pass", &["recruiter"])
        .await;
    sqlx::query("UPDATE users SET display_name = 'Мария Петрова' WHERE id = $1")
        .bind(id)
        .execute(&ctx.db.owner)
        .await
        .unwrap();
    let first = login(&ctx, "maria@acme.io").await;

    // имя поменяли после входа: обновление токена подхватывает актуальное
    sqlx::query("UPDATE users SET display_name = 'Мария Иванова' WHERE id = $1")
        .bind(id)
        .execute(&ctx.db.owner)
        .await
        .unwrap();
    let (s, second) = ctx
        .post(
            "/auth/refresh",
            json!({"refresh_token": first["refresh_token"]}),
        )
        .await;
    assert_eq!(s, 200, "{second}");
    let claims = ctx
        .verifier
        .verify(second["access_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(claims.name.as_deref(), Some("Мария Иванова"));
    ctx.close().await;
}
