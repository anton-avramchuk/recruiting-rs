mod common;

use common::setup;
use identity::application::tenants::TenantStatus;
use platform_auth::Role;
use serde_json::json;

fn login(tenant: &str, email: &str, password: &str) -> serde_json::Value {
    json!({ "tenant": tenant, "email": email, "password": password })
}

#[tokio::test]
async fn successful_login_issues_tokens_with_user_tenant_and_roles() {
    let ctx = setup().await;
    let tid = ctx.dir.add("acme", TenantStatus::Active);
    let uid = ctx
        .seed_user(tid, "anna@acme.io", "s3cret-pass", &["recruiter"])
        .await;

    let (status, body) = ctx
        .post("/auth/login", login("acme", "Anna@Acme.io", "s3cret-pass"))
        .await;
    assert_eq!(status, 200, "{body}");
    let claims = ctx
        .verifier
        .verify(body["access_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(claims.sub, uid);
    assert_eq!(claims.tid, Some(tid));
    assert_eq!(claims.roles, vec![Role::Recruiter]);
    assert!(body["refresh_token"].as_str().unwrap().len() > 20);
    ctx.close().await;
}

#[tokio::test]
async fn wrong_password_and_unknown_email_are_indistinguishable() {
    let ctx = setup().await;
    let tid = ctx.dir.add("acme", TenantStatus::Active);
    ctx.seed_user(tid, "anna@acme.io", "s3cret-pass", &["owner"])
        .await;

    let wrong_pw = ctx
        .post("/auth/login", login("acme", "anna@acme.io", "nope"))
        .await;
    let unknown = ctx
        .post("/auth/login", login("acme", "ghost@acme.io", "nope"))
        .await;
    assert_eq!(wrong_pw.0, 401);
    assert_eq!(wrong_pw, unknown);
    ctx.close().await;
}

#[tokio::test]
async fn unknown_tenant_is_rejected_like_bad_credentials() {
    let ctx = setup().await;
    let (status, _) = ctx.post("/auth/login", login("nope", "a@b.io", "x")).await;
    assert_eq!(status, 401);
    ctx.close().await;
}

#[tokio::test]
async fn suspended_tenant_cannot_log_in() {
    let ctx = setup().await;
    let tid = ctx.dir.add("acme", TenantStatus::Active);
    ctx.seed_user(tid, "anna@acme.io", "s3cret-pass", &["owner"])
        .await;
    ctx.dir.set_status("acme", TenantStatus::Suspended);

    let (status, body) = ctx
        .post("/auth/login", login("acme", "anna@acme.io", "s3cret-pass"))
        .await;
    assert_eq!(status, 403);
    assert!(body.get("access_token").is_none());
    ctx.close().await;
}

#[tokio::test]
async fn blocked_user_cannot_log_in() {
    let ctx = setup().await;
    let tid = ctx.dir.add("acme", TenantStatus::Active);
    let uid = ctx
        .seed_user(tid, "anna@acme.io", "s3cret-pass", &["owner"])
        .await;
    sqlx::query("UPDATE users SET status = 'blocked' WHERE id = $1")
        .bind(uid)
        .execute(&ctx.db.owner)
        .await
        .unwrap();

    let (status, _) = ctx
        .post("/auth/login", login("acme", "anna@acme.io", "s3cret-pass"))
        .await;
    assert_eq!(status, 401);
    ctx.close().await;
}

#[tokio::test]
async fn same_email_in_two_tenants_gives_independent_accounts() {
    let ctx = setup().await;
    let a = ctx.dir.add("a", TenantStatus::Active);
    let b = ctx.dir.add("b", TenantStatus::Active);
    let ua = ctx
        .seed_user(a, "x@mail.io", "pass-for-a", &["owner"])
        .await;
    let ub = ctx
        .seed_user(b, "x@mail.io", "pass-for-b", &["viewer"])
        .await;

    let (s, body_a) = ctx
        .post("/auth/login", login("a", "x@mail.io", "pass-for-a"))
        .await;
    assert_eq!(s, 200);
    assert_eq!(
        ctx.verifier
            .verify(body_a["access_token"].as_str().unwrap())
            .unwrap()
            .sub,
        ua
    );
    let (s, body_b) = ctx
        .post("/auth/login", login("b", "x@mail.io", "pass-for-b"))
        .await;
    assert_eq!(s, 200);
    assert_eq!(
        ctx.verifier
            .verify(body_b["access_token"].as_str().unwrap())
            .unwrap()
            .sub,
        ub
    );
    // пароль одного тенанта не подходит другому
    assert_eq!(
        ctx.post("/auth/login", login("b", "x@mail.io", "pass-for-a"))
            .await
            .0,
        401
    );
    ctx.close().await;
}
