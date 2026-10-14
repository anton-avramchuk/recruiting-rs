mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};

async fn create(app: &App, token: &str, body: Value) -> Value {
    let mut body = body;
    body["confirm_duplicate"] = json!(true);
    let (s, p) = app.send("POST", "/persons", Some(token), Some(body)).await;
    assert_eq!(s, 201, "{p}");
    p
}

async fn patch(app: &App, token: &str, id: &Value, body: Value) -> (u16, Value) {
    app.send(
        "PATCH",
        &format!("/persons/{}", id.as_str().unwrap()),
        Some(token),
        Some(body),
    )
    .await
}

async fn card(app: &App, token: &str, id: &Value) -> Value {
    let (s, p) = app
        .send(
            "GET",
            &format!("/persons/{}", id.as_str().unwrap()),
            Some(token),
            None,
        )
        .await;
    assert_eq!(s, 200);
    p
}

async fn full_person(app: &App, token: &str) -> Value {
    create(
        app,
        token,
        json!({
            "full_name": "Иван Петров", "email": "ivan@mail.ru", "phone": "+7 916 123 45 67",
            "telegram": "ivanov", "whatsapp": "+7 916 123 45 67", "source": "hh.ru",
            "expected_salary": 200000, "summary": "кратко", "skills": ["Rust"]
        }),
    )
    .await
}

#[tokio::test]
async fn omitted_fields_are_untouched_and_given_fields_change() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let p = full_person(&app, &token).await;

    let (s, updated) = patch(
        &app,
        &token,
        &p["id"],
        json!({"full_name": "  Иван   Сергеевич Петров ", "source": "Реферал", "skills": ["SQL", "sql", " Rust "]}),
    )
    .await;
    assert_eq!(s, 200, "{updated}");
    assert_eq!(updated["full_name"], "Иван Сергеевич Петров");
    assert_eq!(updated["source"], "Реферал");
    assert_eq!(updated["skills"], json!(["SQL", "Rust"]));
    // не переданное осталось как было
    assert_eq!(updated["email"], "ivan@mail.ru");
    assert_eq!(updated["phone"], "+79161234567");
    assert_eq!(updated["expected_salary"], 200000);
    assert_eq!(card(&app, &token, &p["id"]).await, updated, "saved");
    app.close().await;
}

#[tokio::test]
async fn null_and_blank_clear_contacts_and_optional_fields() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let p = full_person(&app, &token).await;

    let (s, updated) = patch(
        &app,
        &token,
        &p["id"],
        json!({"phone": null, "telegram": "   ", "expected_salary": null, "summary": ""}),
    )
    .await;
    assert_eq!(s, 200, "{updated}");
    assert_eq!(updated["phone"], Value::Null);
    assert_eq!(updated["telegram"], Value::Null);
    assert_eq!(updated["expected_salary"], Value::Null);
    assert_eq!(updated["summary"], Value::Null);
    assert_eq!(updated["email"], "ivan@mail.ru", "other contacts stay");
    app.close().await;
}

#[tokio::test]
async fn contacts_are_normalized_on_update() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let p = full_person(&app, &token).await;
    let (_, updated) = patch(
        &app,
        &token,
        &p["id"],
        json!({"phone": "8 (903) 555-66-77", "email": " NEW@Mail.ru ", "telegram": "t.me/NewNick"}),
    )
    .await;
    assert_eq!(updated["phone"], "+79035556677");
    assert_eq!(updated["email"], "new@mail.ru");
    assert_eq!(updated["telegram"], "newnick");
    app.close().await;
}

#[tokio::test]
async fn changed_contacts_change_what_counts_as_a_duplicate() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let p = full_person(&app, &token).await;
    patch(
        &app,
        &token,
        &p["id"],
        json!({"email": "moved@mail.ru", "phone": null}),
    )
    .await;

    // новый email теперь занят этим человеком
    let (s, err) = app
        .send(
            "POST",
            "/persons",
            Some(&token),
            Some(json!({"full_name": "Кто-то", "email": "moved@mail.ru"})),
        )
        .await;
    assert_eq!((s, err["duplicate"]["id"].clone()), (409, p["id"].clone()));

    // прежние email и телефон свободны
    for body in [
        json!({"full_name": "Другой А", "email": "ivan@mail.ru"}),
        json!({"full_name": "Другой Б", "phone": "+7 916 123 45 67"}),
    ] {
        let (s, _) = app.send("POST", "/persons", Some(&token), Some(body)).await;
        assert_eq!(s, 201);
    }
    app.close().await;
}

#[tokio::test]
async fn invalid_update_is_a_400_and_changes_nothing() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let p = full_person(&app, &token).await;
    let before = card(&app, &token, &p["id"]).await;

    for (field, body) in [
        (
            "email",
            json!({"full_name": "Новое Имя", "email": "not-an-email"}),
        ),
        ("phone", json!({"source": "x", "phone": "123"})),
        ("full_name", json!({"full_name": "   "})),
        (
            "expected_salary",
            json!({"summary": "новая", "expected_salary": -10}),
        ),
        ("telegram", json!({"telegram": "a"})),
    ] {
        let (s, err) = patch(&app, &token, &p["id"], body).await;
        assert_eq!(s, 400, "{field}: {err}");
        assert_eq!(err["field"], field);
    }
    assert_eq!(
        card(&app, &token, &p["id"]).await,
        before,
        "nothing was partially applied"
    );
    app.close().await;
}

#[tokio::test]
async fn unknown_and_foreign_persons_are_404() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let p = full_person(&app, &token).await;

    let (s, _) = patch(
        &app,
        &token,
        &json!(uuid::Uuid::new_v4().to_string()),
        json!({"source": "x"}),
    )
    .await;
    assert_eq!(s, 404);
    let other = app.token(TenantId::new(), &[Role::Owner]);
    let (s, _) = patch(&app, &other, &p["id"], json!({"source": "hacked"})).await;
    assert_eq!(s, 404);
    assert_eq!(card(&app, &token, &p["id"]).await["source"], "hh.ru");
    app.close().await;
}

#[tokio::test]
async fn viewers_cannot_update() {
    let app = setup().await;
    let tenant = TenantId::new();
    let recruiter = app.token(tenant, &[Role::Recruiter]);
    let viewer = app.token(tenant, &[Role::Viewer]);
    let p = full_person(&app, &recruiter).await;
    let (s, _) = patch(&app, &viewer, &p["id"], json!({"source": "x"})).await;
    assert_eq!(s, 403);
    app.close().await;
}
