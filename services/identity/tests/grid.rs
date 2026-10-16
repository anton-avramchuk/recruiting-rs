//! Список пользователей организации на гриде: страницы, сортировка, фильтры, изоляция, права.
mod common;

use common::{Ctx, setup};
use identity::application::tenants::TenantStatus;
use serde_json::{Value, json};

struct World {
    ctx: Ctx,
    owner_a: String,
    viewer_a: String,
    owner_b: String,
}

async fn token(ctx: &Ctx, tenant: &str, email: &str, password: &str) -> String {
    let (s, body) = ctx
        .post(
            "/auth/login",
            json!({"tenant": tenant, "email": email, "password": password}),
        )
        .await;
    assert_eq!(s, 200, "{body}");
    body["access_token"].as_str().unwrap().to_string()
}

fn encode(raw: &str) -> String {
    raw.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Организация A: владелец, наблюдатель и четыре созданных пользователя.
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
    let w = World {
        ctx,
        owner_a,
        viewer_a,
        owner_b,
    };
    for (email, role, name) in [
        ("anna@a.io", "recruiter", "Анна"),
        ("boris@a.io", "recruiter", "Борис"),
        ("vera@a.io", "viewer", "Вера"),
        ("gleb@a.io", "viewer", "Глеб"),
    ] {
        let (s, body) = w
            .ctx
            .send(
                "POST",
                "/users",
                Some(&w.owner_a),
                json!({"email": email, "password": "long-enough-1", "display_name": name, "roles": [role]}),
            )
            .await;
        assert_eq!(s, 201, "{body}");
        if email == "boris@a.io" || email == "gleb@a.io" {
            let uri = format!("/users/{}", body["id"].as_str().unwrap());
            let (s, _) = w
                .ctx
                .send(
                    "PATCH",
                    &uri,
                    Some(&w.owner_a),
                    json!({"status": "blocked"}),
                )
                .await;
            assert_eq!(s, 200);
        }
    }
    w
}

impl World {
    async fn list(&self, token: &str, query: &str) -> (u16, Value) {
        self.ctx
            .send("GET", &format!("/users{query}"), Some(token), Value::Null)
            .await
    }

    async fn emails(&self, query: &str) -> Vec<String> {
        let (s, body) = self.list(&self.owner_a, query).await;
        assert_eq!(s, 200, "{body}");
        body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u["email"].as_str().unwrap().to_string())
            .collect()
    }
}

fn filter(value: Value) -> String {
    format!("?filter={}", encode(&value.to_string()))
}

#[tokio::test]
async fn the_list_is_a_page_with_a_total_sorted_by_email_by_default() {
    let w = world().await;
    let (s, body) = w.list(&w.owner_a, "").await;
    assert_eq!(s, 200, "{body}");
    assert_eq!(
        (
            body["total"].clone(),
            body["page"].clone(),
            body["page_size"].clone()
        ),
        (json!(6), json!(1), json!(20))
    );
    let emails: Vec<_> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["email"].clone())
        .collect();
    assert_eq!(
        emails,
        [
            "anna@a.io",
            "boris@a.io",
            "gleb@a.io",
            "owner@a.io",
            "vera@a.io",
            "viewer@a.io"
        ]
        .map(|e| json!(e))
    );
    assert!(body["items"][0].get("password_hash").is_none());
    w.ctx.close().await;
}

#[tokio::test]
async fn pages_and_sorting() {
    let w = world().await;
    assert_eq!(
        w.emails("?page=2&page_size=2").await,
        ["gleb@a.io", "owner@a.io"]
    );
    assert_eq!(
        w.emails("?page=3&page_size=2").await,
        ["vera@a.io", "viewer@a.io"]
    );
    assert!(w.emails("?page=4&page_size=2").await.is_empty());
    assert_eq!(
        w.emails("?sort=status:asc,email:desc&page_size=3").await,
        ["viewer@a.io", "vera@a.io", "owner@a.io"]
    );
    let (_, body) = w.list(&w.owner_a, "?page=2&page_size=2").await;
    assert_eq!(body["total"], 6);
    w.ctx.close().await;
}

#[tokio::test]
async fn filters_by_status_role_and_text() {
    let w = world().await;
    let blocked = filter(json!({"field": "status", "operator": "eq", "value": "blocked"}));
    assert_eq!(w.emails(&blocked).await, ["boris@a.io", "gleb@a.io"]);

    let role = filter(json!({"field": "role", "operator": "contains", "value": "viewer"}));
    assert_eq!(
        w.emails(&role).await,
        ["gleb@a.io", "vera@a.io", "viewer@a.io"]
    );

    let both = filter(json!({"logic": "and", "filters": [
        {"field": "status", "operator": "eq", "value": "blocked"},
        {"field": "email", "operator": "contains", "value": "ORIS"},
    ]}));
    assert_eq!(w.emails(&both).await, ["boris@a.io"]);

    let search = filter(json!({"logic": "or", "filters": [
        {"field": "email", "operator": "contains", "value": "ве"},
        {"field": "display_name", "operator": "contains", "value": "ве"},
    ]}));
    assert_eq!(w.emails(&search).await, ["vera@a.io"]);

    let (_, body) = w.list(&w.owner_a, &blocked).await;
    assert_eq!(body["total"], 2);
    w.ctx.close().await;
}

#[tokio::test]
async fn foreign_fields_and_bad_parameters_are_rejected_with_the_parameter() {
    let w = world().await;
    let cases = [
        (
            filter(json!({"field": "password_hash", "operator": "contains", "value": "a"})),
            "filter",
        ),
        ("?sort=password_hash:asc".to_string(), "sort"),
        ("?sort=role:asc".to_string(), "sort"),
        ("?page_size=101".to_string(), "page_size"),
        ("?page=0".to_string(), "page"),
        ("?page=abc".to_string(), "page"),
        ("?filter=%7Bnot-json".to_string(), "filter"),
        (
            filter(json!({"field": "email", "operator": "gt", "value": {"a": 1}})),
            "filter",
        ),
    ];
    for (query, field) in cases {
        let (s, body) = w.list(&w.owner_a, &query).await;
        assert_eq!(s, 400, "{query}: {body}");
        assert_eq!(body["field"], field, "{query}: {body}");
    }
    w.ctx.close().await;
}

#[tokio::test]
async fn a_hostile_filter_value_is_just_text() {
    let w = world().await;
    let evil = filter(
        json!({"field": "email", "operator": "contains", "value": "'; DROP TABLE users; --"}),
    );
    assert!(w.emails(&evil).await.is_empty());
    assert_eq!(w.emails("").await.len(), 6);
    w.ctx.close().await;
}

#[tokio::test]
async fn another_organization_sees_only_its_own_users() {
    let w = world().await;
    let (s, body) = w.list(&w.owner_b, "").await;
    assert_eq!(s, 200);
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["email"], "owner@b.io");
    let probe = filter(json!({"field": "email", "operator": "contains", "value": "@a.io"}));
    assert_eq!(w.list(&w.owner_b, &probe).await.1["total"], 0);
    w.ctx.close().await;
}

#[tokio::test]
async fn one_user_is_available_by_id_within_the_organization() {
    let w = world().await;
    let (_, body) = w.list(&w.owner_a, "").await;
    let anna = body["items"][0]["id"].as_str().unwrap().to_string();

    let (s, one) = w.list(&w.owner_a, &format!("/{anna}")).await;
    assert_eq!(s, 200, "{one}");
    assert_eq!(
        (one["email"].clone(), one["roles"].clone()),
        (json!("anna@a.io"), json!(["recruiter"]))
    );
    assert!(one.get("password_hash").is_none());

    assert_eq!(w.list(&w.owner_b, &format!("/{anna}")).await.0, 404);
    assert_eq!(
        w.list(&w.owner_a, &format!("/{}", uuid::Uuid::new_v4()))
            .await
            .0,
        404
    );
    assert_eq!(w.list(&w.viewer_a, &format!("/{anna}")).await.0, 403);
    w.ctx.close().await;
}

#[tokio::test]
async fn listing_needs_the_user_management_right() {
    let w = world().await;
    assert_eq!(w.list(&w.viewer_a, "").await.0, 403);
    assert_eq!(w.ctx.send("GET", "/users", None, Value::Null).await.0, 401);
    w.ctx.close().await;
}
