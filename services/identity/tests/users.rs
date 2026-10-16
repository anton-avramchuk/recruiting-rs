mod common;

use common::{Ctx, setup};
use identity::application::tenants::TenantStatus;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};

async fn token(ctx: &Ctx, tenant: &str, email: &str, password: &str) -> String {
    let (s, body) = ctx
        .post(
            "/auth/login",
            json!({"tenant": tenant, "email": email, "password": password}),
        )
        .await;
    assert_eq!(s, 200, "login failed: {body}");
    body["access_token"].as_str().unwrap().to_string()
}

struct World {
    ctx: Ctx,
    a: TenantId,
    owner_a: String,
    viewer_a: String,
    owner_b: String,
}

async fn world() -> World {
    let ctx = setup().await;
    let a = ctx.dir.add("a", TenantStatus::Active);
    let b = ctx.dir.add("b", TenantStatus::Active);
    ctx.seed_user(a, "owner@a.io", "password-a1", &["owner"])
        .await;
    ctx.seed_user(a, "viewer@a.io", "password-a2", &["viewer"])
        .await;
    ctx.seed_user(b, "owner@b.io", "password-b1", &["owner"])
        .await;
    let owner_a = token(&ctx, "a", "owner@a.io", "password-a1").await;
    let viewer_a = token(&ctx, "a", "viewer@a.io", "password-a2").await;
    let owner_b = token(&ctx, "b", "owner@b.io", "password-b1").await;
    World {
        ctx,
        a,
        owner_a,
        viewer_a,
        owner_b,
    }
}

fn new_user(email: &str, roles: Value) -> Value {
    json!({"email": email, "password": "long-enough-1", "display_name": "N", "roles": roles})
}

#[tokio::test]
async fn unauthenticated_and_viewer_are_refused() {
    let w = world().await;
    let body = new_user("n@a.io", json!(["viewer"]));
    assert_eq!(
        w.ctx.send("POST", "/users", None, body.clone()).await.0,
        401
    );
    assert_eq!(
        w.ctx
            .send("POST", "/users", Some(&w.viewer_a), body)
            .await
            .0,
        403
    );
    assert_eq!(
        w.ctx
            .send("GET", "/users", Some(&w.viewer_a), Value::Null)
            .await
            .0,
        403
    );
    w.ctx.close().await;
}

#[tokio::test]
async fn owner_creates_user_who_can_log_in() {
    let w = world().await;
    let (s, created) = w
        .ctx
        .send(
            "POST",
            "/users",
            Some(&w.owner_a),
            new_user("New@A.io", json!(["recruiter"])),
        )
        .await;
    assert_eq!(s, 201, "{created}");
    assert_eq!(created["email"], "new@a.io");
    assert_eq!(created["roles"], json!(["recruiter"]));
    assert!(created.get("password_hash").is_none());
    token(&w.ctx, "a", "new@a.io", "long-enough-1").await;
    w.ctx.close().await;
}

#[tokio::test]
async fn invalid_input_is_rejected() {
    let w = world().await;
    let post = |b: Value| w.ctx.send("POST", "/users", Some(&w.owner_a), b);
    assert_eq!(post(new_user("owner@a.io", json!(["viewer"]))).await.0, 409);
    assert_eq!(post(new_user("bad-email", json!(["viewer"]))).await.0, 400);
    assert_eq!(
        post(new_user("x@a.io", json!(["platform_admin"]))).await.0,
        400
    );
    assert_eq!(post(new_user("x@a.io", json!([]))).await.0, 400);
    assert_eq!(post(new_user("x@a.io", json!(["wizard"]))).await.0, 422);
    let mut weak = new_user("x@a.io", json!(["viewer"]));
    weak["password"] = json!("short");
    assert_eq!(post(weak).await.0, 400);
    w.ctx.close().await;
}

#[tokio::test]
async fn owner_blocks_and_changes_roles() {
    let w = world().await;
    let (_, created) = w
        .ctx
        .send(
            "POST",
            "/users",
            Some(&w.owner_a),
            new_user("n@a.io", json!(["viewer"])),
        )
        .await;
    let uri = format!("/users/{}", created["id"].as_str().unwrap());

    let (s, updated) = w
        .ctx
        .send(
            "PATCH",
            &uri,
            Some(&w.owner_a),
            json!({"roles": ["recruiter"]}),
        )
        .await;
    assert_eq!(s, 200, "{updated}");
    assert_eq!(updated["roles"], json!(["recruiter"]));

    let (s, updated) = w
        .ctx
        .send(
            "PATCH",
            &uri,
            Some(&w.owner_a),
            json!({"status": "blocked"}),
        )
        .await;
    assert_eq!((s, &updated["status"]), (200, &json!("blocked")));
    let (s, _) = w
        .ctx
        .post(
            "/auth/login",
            json!({"tenant":"a","email":"n@a.io","password":"long-enough-1"}),
        )
        .await;
    assert_eq!(s, 401);
    w.ctx.close().await;
}

#[tokio::test]
async fn owner_cannot_block_themselves() {
    let w = world().await;
    let (_, list) = w
        .ctx
        .send("GET", "/users", Some(&w.owner_a), Value::Null)
        .await;
    let me = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["email"] == "owner@a.io")
        .unwrap();
    let uri = format!("/users/{}", me["id"].as_str().unwrap());
    let (s, _) = w
        .ctx
        .send(
            "PATCH",
            &uri,
            Some(&w.owner_a),
            json!({"status": "blocked"}),
        )
        .await;
    assert_eq!(s, 409);
    w.ctx.close().await;
}

#[tokio::test]
async fn management_is_confined_to_the_owners_tenant() {
    let w = world().await;
    let (s, list) = w
        .ctx
        .send("GET", "/users", Some(&w.owner_a), Value::Null)
        .await;
    assert_eq!(s, 200);
    let emails: Vec<_> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["email"].clone())
        .collect();
    assert_eq!(emails, [json!("owner@a.io"), json!("viewer@a.io")]);

    // владелец B не видит и не может изменить пользователя A
    let (_, list_a) = w
        .ctx
        .send("GET", "/users", Some(&w.owner_a), Value::Null)
        .await;
    let victim = list_a["items"][1]["id"].as_str().unwrap();
    let (s, _) = w
        .ctx
        .send(
            "PATCH",
            &format!("/users/{victim}"),
            Some(&w.owner_b),
            json!({"status": "blocked"}),
        )
        .await;
    assert_eq!(s, 404);
    let (_, list_b) = w
        .ctx
        .send("GET", "/users", Some(&w.owner_b), Value::Null)
        .await;
    assert_eq!(list_b["items"].as_array().unwrap().len(), 1);
    // и пользователь A по-прежнему активен
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM users WHERE tenant_id = $1 AND status = 'active'")
            .bind(w.a.0)
            .fetch_one(&w.ctx.db.owner)
            .await
            .unwrap();
    assert_eq!(n, 2);
    w.ctx.close().await;
}
