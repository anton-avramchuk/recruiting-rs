mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

async fn post(app: &App, token: &str, target: &str, id: Uuid, body: &str) -> (u16, Value) {
    app.send(
        "POST",
        "/comments",
        Some(token),
        Some(json!({"target_type": target, "target_id": id, "body": body})),
    )
    .await
}

fn list_uri(target: &str, id: Uuid, extra: &str) -> String {
    format!("/comments?target_type={target}&target_id={id}{extra}")
}

#[tokio::test]
async fn a_comment_gets_the_author_from_the_token_and_a_server_time() {
    let app = setup().await;
    let tenant = TenantId::new();
    let user = Uuid::new_v4();
    let token = app.user_token(tenant, user, Some("Мария Петрова"), &[Role::Recruiter]);
    let target = Uuid::new_v4();

    let (s, c) = post(&app, &token, "person", target, "  Перезвонить в пятницу ").await;
    assert_eq!(s, 201, "{c}");
    assert_eq!(c["body"], "Перезвонить в пятницу");
    assert_eq!(c["author_id"], user.to_string());
    assert_eq!(c["author_name"], "Мария Петрова");
    assert_eq!(c["target_type"], "person");
    assert_eq!(c["target_id"], target.to_string());
    assert!(c["created_at"].as_str().unwrap().contains('T'));
    assert_eq!(c["edited_at"], Value::Null);
    app.close().await;
}

#[tokio::test]
async fn a_token_without_a_name_gets_a_neutral_author() {
    let app = setup().await;
    let token = app.user_token(TenantId::new(), Uuid::new_v4(), None, &[Role::Recruiter]);
    let (s, c) = post(&app, &token, "vacancy", Uuid::new_v4(), "x").await;
    assert_eq!(s, 201);
    assert_eq!(c["author_name"], "Пользователь");
    app.close().await;
}

#[tokio::test]
async fn validation_errors_name_the_field() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let target = Uuid::new_v4();
    let too_long = "я".repeat(5001);
    for (kind, body, field) in [
        ("person", "   ", "body"),
        ("person", too_long.as_str(), "body"),
        ("candidate", "текст", "target_type"),
    ] {
        let (s, p) = post(&app, &token, kind, target, body).await;
        assert_eq!((s, p["field"].as_str()), (400, Some(field)), "{p}");
    }
    let (_, page) = app
        .send("GET", &list_uri("person", target, ""), Some(&token), None)
        .await;
    assert_eq!(page["items"].as_array().unwrap().len(), 0);
    app.close().await;
}

#[tokio::test]
async fn the_feed_is_in_creation_order_and_scoped_to_the_target() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    for text in ["первый", "второй", "третий"] {
        post(&app, &token, "vacancy", a, text).await;
    }
    post(&app, &token, "vacancy", b, "чужой объект").await;
    post(&app, &token, "client", a, "тот же id, другой тип").await;

    let (s, page) = app
        .send("GET", &list_uri("vacancy", a, ""), Some(&token), None)
        .await;
    assert_eq!(s, 200);
    let bodies: Vec<_> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["body"].as_str().unwrap())
        .collect();
    assert_eq!(bodies, ["первый", "второй", "третий"]);
    assert_eq!(page["next"], Value::Null);

    let (_, empty) = app
        .send("GET", &list_uri("person", a, ""), Some(&token), None)
        .await;
    assert_eq!(empty["items"].as_array().unwrap().len(), 0);
    app.close().await;
}

#[tokio::test]
async fn pages_follow_the_cursor_without_gaps_or_repeats() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let target = Uuid::new_v4();
    for i in 1..=5 {
        post(&app, &token, "person", target, &format!("c{i}")).await;
    }
    let mut seen = Vec::new();
    let mut after = String::new();
    loop {
        let (s, page) = app
            .send(
                "GET",
                &list_uri("person", target, &format!("&limit=2{after}")),
                Some(&token),
                None,
            )
            .await;
        assert_eq!(s, 200, "{page}");
        for c in page["items"].as_array().unwrap() {
            seen.push(c["body"].as_str().unwrap().to_string());
        }
        match page["next"].as_str() {
            Some(cursor) => after = format!("&after={cursor}"),
            None => break,
        }
    }
    assert_eq!(seen, ["c1", "c2", "c3", "c4", "c5"]);

    for extra in ["&after=мусор", "&limit=0"] {
        let (s, _) = app
            .send(
                "GET",
                &list_uri("person", target, extra),
                Some(&token),
                None,
            )
            .await;
        assert_eq!(s, 400, "{extra}");
    }
    let (s, _) = app
        .send("GET", &list_uri("nothing", target, ""), Some(&token), None)
        .await;
    assert_eq!(s, 400);
    app.close().await;
}

#[tokio::test]
async fn another_organization_sees_nothing() {
    let app = setup().await;
    let a = app.token(TenantId::new(), &[Role::Recruiter]);
    let b = app.token(TenantId::new(), &[Role::Recruiter]);
    let target = Uuid::new_v4();
    let (_, c) = post(&app, &a, "person", target, "секрет").await;
    let (_, page) = app
        .send("GET", &list_uri("person", target, ""), Some(&b), None)
        .await;
    assert_eq!(page["items"].as_array().unwrap().len(), 0);
    let id = c["id"].as_str().unwrap();
    let (s, _) = app
        .send(
            "PATCH",
            &format!("/comments/{id}"),
            Some(&b),
            Some(json!({"body": "взлом"})),
        )
        .await;
    assert_eq!(s, 404);
    let (s, _) = app
        .send("DELETE", &format!("/comments/{id}"), Some(&b), None)
        .await;
    assert_eq!(s, 404);
    app.close().await;
}
