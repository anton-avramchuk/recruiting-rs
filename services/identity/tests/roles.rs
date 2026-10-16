//! Собственные роли организации: CRUD, назначение пользователям, права в токене.
mod common;

use common::{Ctx, setup};
use identity::application::tenants::TenantStatus;
use platform_auth::{Permission, Role};
use serde_json::{Value, json};

struct World {
    ctx: Ctx,
    owner: String,
    owner_b: String,
}

async fn login(ctx: &Ctx, tenant: &str, email: &str, password: &str) -> Value {
    let (s, body) = ctx
        .post(
            "/auth/login",
            json!({"tenant": tenant, "email": email, "password": password}),
        )
        .await;
    assert_eq!(s, 200, "{body}");
    body
}

fn access(tokens: &Value) -> String {
    tokens["access_token"].as_str().unwrap().to_string()
}

async fn world() -> World {
    let ctx = setup().await;
    let a = ctx.dir.add("a", TenantStatus::Active);
    let b = ctx.dir.add("b", TenantStatus::Active);
    ctx.seed_user(a, "owner@a.io", "password-a1", &["owner"])
        .await;
    ctx.seed_user(b, "owner@b.io", "password-b1", &["owner"])
        .await;
    let owner = access(&login(&ctx, "a", "owner@a.io", "password-a1").await);
    let owner_b = access(&login(&ctx, "b", "owner@b.io", "password-b1").await);
    World {
        ctx,
        owner,
        owner_b,
    }
}

impl World {
    async fn call(&self, method: &str, uri: &str, token: &str, body: Value) -> (u16, Value) {
        self.ctx.send(method, uri, Some(token), body).await
    }

    async fn role(&self, name: &str, perms: &[&str]) -> Value {
        let (s, body) = self
            .call(
                "POST",
                "/roles",
                &self.owner,
                json!({"name": name, "permissions": perms}),
            )
            .await;
        assert_eq!(s, 201, "{body}");
        body
    }

    async fn try_role(&self, name: &str, perms: Value) -> (u16, Value) {
        self.call(
            "POST",
            "/roles",
            &self.owner,
            json!({"name": name, "permissions": perms}),
        )
        .await
    }

    async fn user_with_role(&self, email: &str, role_id: &str) -> Value {
        let (s, body) = self
            .call(
                "POST",
                "/users",
                &self.owner,
                json!({"email": email, "password": "long-enough-1", "role_id": role_id}),
            )
            .await;
        assert_eq!(s, 201, "{body}");
        body
    }
}

fn id(v: &Value) -> String {
    v["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn the_list_has_built_in_roles_then_custom_ones_with_user_counts() {
    let w = world().await;
    let role = w.role("Стажёр", &["read_data", "write_data"]).await;
    assert_eq!(
        (role["kind"].clone(), role["users"].clone()),
        (json!("custom"), json!(0))
    );
    w.user_with_role("intern@a.io", &id(&role)).await;

    let (s, list) = w.call("GET", "/roles", &w.owner, Value::Null).await;
    assert_eq!(s, 200, "{list}");
    let kinds: Vec<_> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["kind"].clone())
        .collect();
    assert_eq!(
        kinds,
        [
            json!("builtin"),
            json!("builtin"),
            json!("builtin"),
            json!("custom")
        ]
    );
    let builtin_names: Vec<_> = list.as_array().unwrap()[..3]
        .iter()
        .map(|r| r["name"].clone())
        .collect();
    assert_eq!(
        builtin_names,
        [json!("Владелец"), json!("Рекрутёр"), json!("Наблюдатель")]
    );
    let custom = &list[3];
    assert_eq!(custom["name"], "Стажёр");
    assert_eq!(custom["permissions"], json!(["read_data", "write_data"]));
    assert_eq!(custom["users"], 1);
    w.ctx.close().await;
}

#[tokio::test]
async fn names_and_permissions_are_validated() {
    let w = world().await;
    w.role("Стажёр", &["read_data"]).await;
    // занятое название (без учёта регистра) и названия встроенных ролей
    assert_eq!(w.try_role("стажёр", json!(["read_data"])).await.0, 409);
    assert_eq!(w.try_role("Владелец", json!(["read_data"])).await.0, 409);
    assert_eq!(w.try_role("viewer", json!(["read_data"])).await.0, 409);
    // название
    assert_eq!(w.try_role("  ", json!(["read_data"])).await.0, 400);
    assert_eq!(
        w.try_role(&"я".repeat(61), json!(["read_data"])).await.0,
        400
    );
    // права
    assert_eq!(w.try_role("A", json!([])).await.0, 400);
    assert_eq!(w.try_role("B", json!(["write_data"])).await.0, 400);
    assert_eq!(w.try_role("C", json!(["delete_data"])).await.0, 400);
    assert_eq!(w.try_role("D", json!(["read_data", "sudo"])).await.0, 400);
    assert_eq!(
        w.try_role("E", json!(["read_data", "manage_users"]))
            .await
            .0,
        400
    );
    assert_eq!(
        w.try_role("F", json!(["read_data", "write_data", "delete_data"]))
            .await
            .0,
        201
    );
    w.ctx.close().await;
}

#[tokio::test]
async fn a_role_can_be_renamed_and_its_permissions_changed() {
    let w = world().await;
    let role = w.role("Стажёр", &["read_data", "write_data"]).await;
    let uri = format!("/roles/{}", id(&role));
    let (s, body) = w
        .call("PATCH", &uri, &w.owner, json!({"name": "Практикант"}))
        .await;
    assert_eq!((s, body["name"].clone()), (200, json!("Практикант")));
    let (s, body) = w
        .call(
            "PATCH",
            &uri,
            &w.owner,
            json!({"permissions": ["read_data"]}),
        )
        .await;
    assert_eq!(
        (s, body["permissions"].clone()),
        (200, json!(["read_data"]))
    );
    assert_eq!(
        w.call(
            "PATCH",
            &uri,
            &w.owner,
            json!({"permissions": ["write_data"]})
        )
        .await
        .0,
        400
    );
    let other = w.role("Другая", &["read_data"]).await;
    assert_eq!(
        w.call("PATCH", &uri, &w.owner, json!({"name": "ДРУГАЯ"}))
            .await
            .0,
        409
    );
    let _ = other;
    w.ctx.close().await;
}

#[tokio::test]
async fn built_in_and_unknown_roles_cannot_be_changed_or_removed() {
    let w = world().await;
    for fake in ["owner", "00000000-0000-0000-0000-000000000000", "not-an-id"] {
        let uri = format!("/roles/{fake}");
        let (patch, _) = w.call("PATCH", &uri, &w.owner, json!({"name": "X"})).await;
        let (delete, _) = w.call("DELETE", &uri, &w.owner, Value::Null).await;
        assert!(matches!(patch, 404 | 400), "{fake}: {patch}");
        assert!(matches!(delete, 404 | 400), "{fake}: {delete}");
    }
    w.ctx.close().await;
}

#[tokio::test]
async fn an_unused_role_is_deleted_but_an_assigned_one_is_not() {
    let w = world().await;
    let free = w.role("Свободная", &["read_data"]).await;
    let used = w.role("Занятая", &["read_data"]).await;
    w.user_with_role("u@a.io", &id(&used)).await;

    assert_eq!(
        w.call(
            "DELETE",
            &format!("/roles/{}", id(&free)),
            &w.owner,
            Value::Null
        )
        .await
        .0,
        204
    );
    let (s, body) = w
        .call(
            "DELETE",
            &format!("/roles/{}", id(&used)),
            &w.owner,
            Value::Null,
        )
        .await;
    assert_eq!(s, 409, "{body}");
    assert!(body["detail"].as_str().unwrap().contains('1'), "{body}");
    let (_, list) = w.call("GET", "/roles", &w.owner, Value::Null).await;
    assert_eq!(list.as_array().unwrap().len(), 4);
    w.ctx.close().await;
}

#[tokio::test]
async fn only_owners_manage_roles_and_organizations_are_isolated() {
    let w = world().await;
    let role = w
        .role("Стажёр", &["read_data", "write_data", "delete_data"])
        .await;
    w.user_with_role("intern@a.io", &id(&role)).await;
    let intern = access(&login(&w.ctx, "a", "intern@a.io", "long-enough-1").await);

    // собственная роль, даже с полным набором прав, не управляет пользователями и ролями
    assert_eq!(w.call("GET", "/roles", &intern, Value::Null).await.0, 403);
    assert_eq!(
        w.call(
            "POST",
            "/roles",
            &intern,
            json!({"name": "X", "permissions": ["read_data"]})
        )
        .await
        .0,
        403
    );
    assert_eq!(w.call("GET", "/users", &intern, Value::Null).await.0, 403);
    assert_eq!(w.ctx.send("GET", "/roles", None, Value::Null).await.0, 401);

    // другая организация не видит и не меняет роль
    let (_, list) = w.call("GET", "/roles", &w.owner_b, Value::Null).await;
    assert_eq!(list.as_array().unwrap().len(), 3);
    let uri = format!("/roles/{}", id(&role));
    assert_eq!(
        w.call("PATCH", &uri, &w.owner_b, json!({"name": "Взлом"}))
            .await
            .0,
        404
    );
    assert_eq!(w.call("DELETE", &uri, &w.owner_b, Value::Null).await.0, 404);
    w.ctx.close().await;
}

#[tokio::test]
async fn the_token_of_a_custom_role_carries_exactly_its_permissions() {
    let w = world().await;
    let role = w.role("Стажёр", &["read_data", "write_data"]).await;
    let created = w.user_with_role("intern@a.io", &id(&role)).await;
    assert_eq!(created["role_id"], role["id"]);
    assert_eq!(created["role_name"], "Стажёр");
    assert_eq!(created["roles"], json!([]));

    let tokens = login(&w.ctx, "a", "intern@a.io", "long-enough-1").await;
    let claims = w.ctx.verifier.verify(&access(&tokens)).unwrap();
    assert!(claims.roles.is_empty());
    assert_eq!(
        claims.perms,
        vec![Permission::ReadData, Permission::WriteData]
    );
    assert!(claims.has(Permission::WriteData));
    assert!(!claims.has(Permission::DeleteData) && !claims.has(Permission::ManageUsers));

    // refresh сохраняет права
    let (s, next) = w
        .ctx
        .post(
            "/auth/refresh",
            json!({"refresh_token": tokens["refresh_token"]}),
        )
        .await;
    assert_eq!(s, 200);
    let claims = w.ctx.verifier.verify(&access(&next)).unwrap();
    assert_eq!(claims.perms.len(), 2);
    w.ctx.close().await;
}

#[tokio::test]
async fn built_in_and_custom_assignments_replace_each_other() {
    let w = world().await;
    let role = w.role("Стажёр", &["read_data"]).await;
    let created = w.user_with_role("u@a.io", &id(&role)).await;
    let uri = format!("/users/{}", id(&created));

    let (s, body) = w
        .call("PATCH", &uri, &w.owner, json!({"roles": ["recruiter"]}))
        .await;
    assert_eq!(s, 200, "{body}");
    assert_eq!(
        (body["roles"].clone(), body["role_id"].clone()),
        (json!(["recruiter"]), Value::Null)
    );
    let claims = w
        .ctx
        .verifier
        .verify(&access(
            &login(&w.ctx, "a", "u@a.io", "long-enough-1").await,
        ))
        .unwrap();
    assert_eq!(
        (claims.roles, claims.perms.len()),
        (vec![Role::Recruiter], 0)
    );

    let (s, body) = w
        .call("PATCH", &uri, &w.owner, json!({"role_id": id(&role)}))
        .await;
    assert_eq!(s, 200, "{body}");
    assert_eq!(
        (body["roles"].clone(), body["role_name"].clone()),
        (json!([]), json!("Стажёр"))
    );

    // вместе нельзя, ни одного при создании - тоже
    assert_eq!(
        w.call(
            "PATCH",
            &uri,
            &w.owner,
            json!({"roles": ["viewer"], "role_id": id(&role)})
        )
        .await
        .0,
        400
    );
    let neither = json!({"email": "x@a.io", "password": "long-enough-1"});
    assert_eq!(w.call("POST", "/users", &w.owner, neither).await.0, 400);
    w.ctx.close().await;
}

#[tokio::test]
async fn a_role_of_another_organization_or_a_missing_one_is_not_assignable() {
    let w = world().await;
    let (_, foreign) = w
        .call(
            "POST",
            "/roles",
            &w.owner_b,
            json!({"name": "Чужая", "permissions": ["read_data"]}),
        )
        .await;
    for role_id in [
        id(&foreign),
        "00000000-0000-0000-0000-000000000000".to_string(),
    ] {
        let (s, _) = w
            .call(
                "POST",
                "/users",
                &w.owner,
                json!({"email": "u@a.io", "password": "long-enough-1", "role_id": role_id}),
            )
            .await;
        assert_eq!(s, 400, "{role_id}");
    }
    w.ctx.close().await;
}

#[tokio::test]
async fn the_only_owner_cannot_swap_to_a_custom_role() {
    let w = world().await;
    let role = w.role("Стажёр", &["read_data"]).await;
    let (_, list) = w.call("GET", "/users", &w.owner, Value::Null).await;
    let me = id(&list["items"][0]);
    let (s, _) = w
        .call(
            "PATCH",
            &format!("/users/{me}"),
            &w.owner,
            json!({"role_id": id(&role)}),
        )
        .await;
    assert_eq!(s, 409);
    w.ctx.close().await;
}

#[tokio::test]
async fn changed_permissions_revoke_sessions_but_a_rename_does_not() {
    let w = world().await;
    let role = w.role("Стажёр", &["read_data", "write_data"]).await;
    w.user_with_role("u@a.io", &id(&role)).await;
    let uri = format!("/roles/{}", id(&role));
    let refresh = |tokens: Value| {
        let w = &w;
        async move {
            w.ctx
                .post(
                    "/auth/refresh",
                    json!({"refresh_token": tokens["refresh_token"]}),
                )
                .await
                .0
        }
    };

    let tokens = login(&w.ctx, "a", "u@a.io", "long-enough-1").await;
    assert_eq!(
        w.call("PATCH", &uri, &w.owner, json!({"name": "Практикант"}))
            .await
            .0,
        200
    );
    let tokens = {
        let (s, next) = w
            .ctx
            .post(
                "/auth/refresh",
                json!({"refresh_token": tokens["refresh_token"]}),
            )
            .await;
        assert_eq!(s, 200, "переименование не отзывает сессии");
        next
    };
    assert_eq!(
        w.call(
            "PATCH",
            &uri,
            &w.owner,
            json!({"permissions": ["read_data"]})
        )
        .await
        .0,
        200
    );
    assert_eq!(refresh(tokens).await, 401);

    // новый вход уже с новыми правами
    let claims = w
        .ctx
        .verifier
        .verify(&access(
            &login(&w.ctx, "a", "u@a.io", "long-enough-1").await,
        ))
        .unwrap();
    assert_eq!(claims.perms, vec![Permission::ReadData]);
    w.ctx.close().await;
}
