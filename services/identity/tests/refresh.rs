mod common;

use common::{Ctx, setup};
use identity::application::tenants::TenantStatus;
use serde_json::{Value, json};

async fn logged_in() -> (Ctx, Value) {
    let ctx = setup().await;
    let tid = ctx.dir.add("acme", TenantStatus::Active);
    ctx.seed_user(tid, "anna@acme.io", "s3cret-pass", &["recruiter"])
        .await;
    let (s, tokens) = ctx
        .post(
            "/auth/login",
            json!({"tenant": "acme", "email": "anna@acme.io", "password": "s3cret-pass"}),
        )
        .await;
    assert_eq!(s, 200);
    (ctx, tokens)
}

fn refresh_body(tokens: &Value) -> Value {
    json!({ "refresh_token": tokens["refresh_token"] })
}

#[tokio::test]
async fn refresh_rotates_the_pair_and_invalidates_the_old_token() {
    let (ctx, first) = logged_in().await;

    let (s, second) = ctx.post("/auth/refresh", refresh_body(&first)).await;
    assert_eq!(s, 200, "{second}");
    assert_ne!(second["refresh_token"], first["refresh_token"]);
    ctx.verifier
        .verify(second["access_token"].as_str().unwrap())
        .unwrap();

    // повторное использование старого токена отклоняется
    let (s, _) = ctx.post("/auth/refresh", refresh_body(&first)).await;
    assert_eq!(s, 401);
    ctx.close().await;
}

#[tokio::test]
async fn reuse_of_a_rotated_token_revokes_the_whole_chain() {
    let (ctx, first) = logged_in().await;
    let (_, second) = ctx.post("/auth/refresh", refresh_body(&first)).await;

    assert_eq!(ctx.post("/auth/refresh", refresh_body(&first)).await.0, 401); // кража
    // легитимный держатель нового токена тоже должен войти заново
    assert_eq!(
        ctx.post("/auth/refresh", refresh_body(&second)).await.0,
        401
    );
    ctx.close().await;
}

#[tokio::test]
async fn logout_revokes_the_refresh_token() {
    let (ctx, tokens) = logged_in().await;

    let (s, _) = ctx.post("/auth/logout", refresh_body(&tokens)).await;
    assert_eq!(s, 204);
    assert_eq!(
        ctx.post("/auth/refresh", refresh_body(&tokens)).await.0,
        401
    );
    // повторный выход идемпотентен
    assert_eq!(ctx.post("/auth/logout", refresh_body(&tokens)).await.0, 204);
    ctx.close().await;
}

#[tokio::test]
async fn garbage_or_foreign_refresh_tokens_are_rejected() {
    let (ctx, _) = logged_in().await;
    for bad in [
        "garbage",
        "00000000-0000-0000-0000-000000000000.abc",
        "not-a-uuid.abc",
    ] {
        let (s, _) = ctx
            .post("/auth/refresh", json!({ "refresh_token": bad }))
            .await;
        assert_eq!(s, 401, "{bad}");
    }
    ctx.close().await;
}

#[tokio::test]
async fn refresh_is_refused_for_blocked_user() {
    let (ctx, tokens) = logged_in().await;
    sqlx::query("UPDATE users SET status = 'blocked'")
        .execute(&ctx.db.owner)
        .await
        .unwrap();
    assert_eq!(
        ctx.post("/auth/refresh", refresh_body(&tokens)).await.0,
        401
    );
    ctx.close().await;
}
