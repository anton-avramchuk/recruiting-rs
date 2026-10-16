//! Изменение доступа пользователей: защита владельца, сброс пароля, отзыв сессий.
mod common;

use common::{Ctx, setup};
use identity::application::tenants::TenantStatus;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};

struct World {
    ctx: Ctx,
    owner: String,
}

async fn login(ctx: &Ctx, tenant: &str, email: &str, password: &str) -> Value {
    let (s, body) = ctx
        .post(
            "/auth/login",
            json!({"tenant": tenant, "email": email, "password": password}),
        )
        .await;
    assert_eq!(s, 200, "login failed: {body}");
    body
}

async fn world() -> World {
    let ctx = setup().await;
    let a = ctx.dir.add("a", TenantStatus::Active);
    ctx.seed_user(a, "owner@a.io", "password-a1", &["owner"])
        .await;
    let owner = login(&ctx, "a", "owner@a.io", "password-a1").await["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    World { ctx, owner }
}

impl World {
    async fn create(&self, email: &str, role: &str) -> String {
        let (s, body) = self
            .ctx
            .send(
                "POST",
                "/users",
                Some(&self.owner),
                json!({"email": email, "password": "long-enough-1", "display_name": "N", "roles": [role]}),
            )
            .await;
        assert_eq!(s, 201, "{body}");
        body["id"].as_str().unwrap().to_string()
    }

    async fn patch_as(&self, token: &str, id: &str, body: Value) -> (u16, Value) {
        self.ctx
            .send("PATCH", &format!("/users/{id}"), Some(token), body)
            .await
    }

    async fn patch(&self, id: &str, body: Value) -> (u16, Value) {
        self.patch_as(&self.owner, id, body).await
    }

    async fn owner_id(&self) -> String {
        let (_, list) = self
            .ctx
            .send("GET", "/users", Some(&self.owner), Value::Null)
            .await;
        list.as_array()
            .unwrap()
            .iter()
            .find(|u| u["email"] == "owner@a.io")
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    async fn refresh(&self, tokens: &Value) -> u16 {
        self.ctx
            .post(
                "/auth/refresh",
                json!({"refresh_token": tokens["refresh_token"]}),
            )
            .await
            .0
    }
}

#[tokio::test]
async fn the_only_owner_cannot_demote_themselves() {
    let w = world().await;
    let me = w.owner_id().await;
    let (s, _) = w.patch(&me, json!({"roles": ["recruiter"]})).await;
    assert_eq!(s, 409);
    let (_, list) = w
        .ctx
        .send("GET", "/users", Some(&w.owner), Value::Null)
        .await;
    assert_eq!(list[0]["roles"], json!(["owner"]));
    w.ctx.close().await;
}

#[tokio::test]
async fn with_two_owners_one_may_step_down_but_not_the_last() {
    let w = world().await;
    let second = w.create("second@a.io", "owner").await;
    let me = w.owner_id().await;
    let (s, body) = w.patch(&me, json!({"roles": ["recruiter"]})).await;
    assert_eq!(s, 200, "{body}");

    // теперь владелец - только второй: старый токен ещё действителен, но понизить второго нельзя
    let second_token = login(&w.ctx, "a", "second@a.io", "long-enough-1").await["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    let (s, _) = w
        .patch_as(&second_token, &second, json!({"roles": ["viewer"]}))
        .await;
    assert_eq!(s, 409);
    w.ctx.close().await;
}

#[tokio::test]
async fn a_blocked_owner_does_not_count_as_an_owner() {
    let w = world().await;
    let second = w.create("second@a.io", "owner").await;
    assert_eq!(w.patch(&second, json!({"status": "blocked"})).await.0, 200);
    let me = w.owner_id().await;
    let (s, _) = w.patch(&me, json!({"roles": ["viewer"]})).await;
    assert_eq!(s, 409);
    w.ctx.close().await;
}

#[tokio::test]
async fn blocking_the_last_active_owner_is_refused() {
    let w = world().await;
    let second = w.create("second@a.io", "owner").await;
    let me = w.owner_id().await;
    // владелец блокирует второго, затем второй уже заблокирован: блокировка единственного активного отклоняется
    assert_eq!(w.patch(&second, json!({"status": "blocked"})).await.0, 200);
    let (s, _) = w.patch(&me, json!({"status": "blocked"})).await;
    assert_eq!(s, 409);
    w.ctx.close().await;
}

#[tokio::test]
async fn parallel_step_downs_cannot_leave_the_tenant_without_an_owner() {
    let w = world().await;
    let second = w.create("second@a.io", "owner").await;
    let me = w.owner_id().await;
    let second_token = login(&w.ctx, "a", "second@a.io", "long-enough-1").await["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    let (first, other) = tokio::join!(
        w.patch_as(&w.owner, &me, json!({"roles": ["viewer"]})),
        w.patch_as(&second_token, &second, json!({"roles": ["viewer"]})),
    );
    let mut codes = [first.0, other.0];
    codes.sort_unstable();
    assert_eq!(codes, [200, 409]);
    let owners: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM users WHERE 'owner' = ANY(roles) AND status = 'active'",
    )
    .fetch_one(&w.ctx.db.owner)
    .await
    .unwrap();
    assert_eq!(owners, 1);
    w.ctx.close().await;
}

#[tokio::test]
async fn the_owner_resets_a_password() {
    let w = world().await;
    let id = w.create("n@a.io", "recruiter").await;
    let (s, body) = w.patch(&id, json!({"password": "brand-new-pass"})).await;
    assert_eq!(s, 200, "{body}");
    assert!(body.get("password").is_none() && body.get("password_hash").is_none());

    let (s, _) = w
        .ctx
        .post(
            "/auth/login",
            json!({"tenant":"a","email":"n@a.io","password":"long-enough-1"}),
        )
        .await;
    assert_eq!(s, 401);
    login(&w.ctx, "a", "n@a.io", "brand-new-pass").await;
    w.ctx.close().await;
}

#[tokio::test]
async fn a_weak_password_is_rejected_and_nothing_changes() {
    let w = world().await;
    let id = w.create("n@a.io", "recruiter").await;
    let (s, _) = w.patch(&id, json!({"password": "short"})).await;
    assert_eq!(s, 400);
    login(&w.ctx, "a", "n@a.io", "long-enough-1").await;
    w.ctx.close().await;
}

#[tokio::test]
async fn another_organization_cannot_reset_a_password() {
    let w = world().await;
    let b: TenantId = w.ctx.dir.add("b", TenantStatus::Active);
    w.ctx
        .seed_user(b, "owner@b.io", "password-b1", &["owner"])
        .await;
    let owner_b = login(&w.ctx, "b", "owner@b.io", "password-b1").await["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    let id = w.create("n@a.io", "recruiter").await;
    let (s, _) = w
        .patch_as(&owner_b, &id, json!({"password": "hijacked-pass"}))
        .await;
    assert_eq!(s, 404);
    login(&w.ctx, "a", "n@a.io", "long-enough-1").await;
    w.ctx.close().await;
}

#[tokio::test]
async fn blocking_revokes_refresh_tokens() {
    let w = world().await;
    let id = w.create("n@a.io", "recruiter").await;
    let tokens = login(&w.ctx, "a", "n@a.io", "long-enough-1").await;
    assert_eq!(w.patch(&id, json!({"status": "blocked"})).await.0, 200);
    assert_eq!(w.refresh(&tokens).await, 401);
    // после разблокировки старые токены остаются отозванными, вход возможен заново
    assert_eq!(w.patch(&id, json!({"status": "active"})).await.0, 200);
    assert_eq!(w.refresh(&tokens).await, 401);
    login(&w.ctx, "a", "n@a.io", "long-enough-1").await;
    w.ctx.close().await;
}

#[tokio::test]
async fn changing_roles_revokes_refresh_tokens_and_new_login_has_the_new_role() {
    let w = world().await;
    let id = w.create("n@a.io", "viewer").await;
    let tokens = login(&w.ctx, "a", "n@a.io", "long-enough-1").await;
    assert_eq!(w.patch(&id, json!({"roles": ["recruiter"]})).await.0, 200);
    assert_eq!(w.refresh(&tokens).await, 401);
    let fresh = login(&w.ctx, "a", "n@a.io", "long-enough-1").await;
    let claims = w
        .ctx
        .verifier
        .verify(fresh["access_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        claims.roles,
        vec![platform_auth::Role::Recruiter],
        "новая роль в токене"
    );
    w.ctx.close().await;
}

#[tokio::test]
async fn resetting_a_password_revokes_refresh_tokens() {
    let w = world().await;
    let id = w.create("n@a.io", "recruiter").await;
    let tokens = login(&w.ctx, "a", "n@a.io", "long-enough-1").await;
    assert_eq!(
        w.patch(&id, json!({"password": "brand-new-pass"})).await.0,
        200
    );
    assert_eq!(w.refresh(&tokens).await, 401);
    w.ctx.close().await;
}

#[tokio::test]
async fn renaming_keeps_sessions() {
    let w = world().await;
    let id = w.create("n@a.io", "recruiter").await;
    let tokens = login(&w.ctx, "a", "n@a.io", "long-enough-1").await;
    let (s, body) = w.patch(&id, json!({"display_name": "Новое имя"})).await;
    assert_eq!((s, &body["display_name"]), (200, &json!("Новое имя")));
    assert_eq!(w.refresh(&tokens).await, 200);
    w.ctx.close().await;
}
