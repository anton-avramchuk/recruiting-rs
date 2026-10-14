mod common;

use common::{ADMIN_EMAIL, ADMIN_PASSWORD, setup};
use platform_auth::{Audience, KeyPair, Role, TokenVerifier};
use serde_json::json;

#[tokio::test]
async fn seeded_admin_logs_in_and_gets_a_platform_token() {
    let ctx = setup().await;
    let (s, body) = ctx
        .post(
            "/auth/login",
            json!({"email": "Root@Platform.io", "password": ADMIN_PASSWORD}),
        )
        .await;
    assert_eq!(s, 200, "{body}");
    let claims = TokenVerifier::new(&ctx.keys.public_key(), Audience::Platform)
        .verify(body["access_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(claims.roles, vec![Role::PlatformAdmin]);
    assert_eq!(claims.tid, None);
    ctx.close().await;
}

#[tokio::test]
async fn bad_credentials_are_indistinguishable() {
    let ctx = setup().await;
    let wrong = ctx
        .post(
            "/auth/login",
            json!({"email": ADMIN_EMAIL, "password": "nope"}),
        )
        .await;
    let unknown = ctx
        .post(
            "/auth/login",
            json!({"email": "ghost@platform.io", "password": "nope"}),
        )
        .await;
    assert_eq!(wrong.0, 401);
    assert_eq!(wrong, unknown);
    ctx.close().await;
}

#[tokio::test]
async fn me_requires_a_platform_admin_token() {
    let ctx = setup().await;
    assert_eq!(ctx.send("GET", "/me", None, json!(null)).await.0, 401);

    let token = ctx.admin_token().await;
    let (s, me) = ctx.send("GET", "/me", Some(&token), json!(null)).await;
    assert_eq!(s, 200);
    assert_eq!(me["email"], ADMIN_EMAIL);
    ctx.close().await;
}

#[tokio::test]
async fn tenant_tokens_do_not_open_admin_endpoints() {
    let ctx = setup().await;
    // подпись доверенная, но audience чужая -> 403
    let tenant_token = ctx.forge(Audience::Tenant, vec![Role::Owner]);
    assert_eq!(
        ctx.send("GET", "/me", Some(&tenant_token), json!(null))
            .await
            .0,
        403
    );
    // токен identity подписан другим ключом -> 401
    let identity_token = platform_auth::TokenSigner::new(&KeyPair::generate("identity"))
        .sign(&platform_auth::Claims::new(
            uuid::Uuid::new_v4(),
            Some(platform_core::tenant::TenantId::new()),
            vec![Role::Owner],
            Audience::Tenant,
            600,
        ))
        .unwrap();
    assert_eq!(
        ctx.send("GET", "/me", Some(&identity_token), json!(null))
            .await
            .0,
        401
    );
    ctx.close().await;
}

#[tokio::test]
async fn platform_token_without_the_admin_role_is_forbidden() {
    let ctx = setup().await;
    let token = ctx.forge(Audience::Platform, vec![Role::Viewer]);
    assert_eq!(
        ctx.send("GET", "/me", Some(&token), json!(null)).await.0,
        403
    );
    ctx.close().await;
}
