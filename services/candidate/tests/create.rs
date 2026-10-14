mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};

async fn create(app: &App, token: &str, body: Value) -> (u16, Value) {
    app.send("POST", "/persons", Some(token), Some(body)).await
}

#[tokio::test]
async fn minimal_creation_collapses_the_name() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);

    let (s, person) = create(&app, &token, json!({"full_name": "  Иван   Петров "})).await;
    assert_eq!(s, 201, "{person}");
    assert_eq!(person["full_name"], "Иван Петров");
    assert_eq!(person["experience_years"], 0);
    assert_eq!(person["phone"], Value::Null);
    assert!(person["id"].as_str().is_some());
    assert_eq!(app.count(tenant).await, 1);
    app.close().await;
}

#[tokio::test]
async fn full_creation_normalizes_every_field() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Owner]);
    let (s, p) = create(
        &app,
        &token,
        json!({
            "full_name": "Анна Смирнова",
            "phone": "8 (916) 123-45-67",
            "email": " Anna@Mail.RU ",
            "telegram": "t.me/AnnaS_dev",
            "whatsapp": "+7 916 123 45 67",
            "source": "  hh.ru ",
            "expected_salary": 250000,
            "summary": "Backend, 5 лет",
            "skills": ["Rust", " rust ", "SQL"],
            "declared_experience_years": 5
        }),
    )
    .await;
    assert_eq!(s, 201, "{p}");
    assert_eq!(p["phone"], "+79161234567");
    assert_eq!(p["email"], "anna@mail.ru");
    assert_eq!(p["telegram"], "annas_dev");
    assert_eq!(p["whatsapp"], "+79161234567");
    assert_eq!(p["source"], "hh.ru");
    assert_eq!(p["expected_salary"], 250000);
    assert_eq!(p["skills"], json!(["Rust", "SQL"]));
    assert_eq!(
        p["experience_years"], 5,
        "declared experience without history"
    );
    app.close().await;
}

#[tokio::test]
async fn invalid_input_is_a_400_naming_the_field_and_writes_nothing() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);

    for (field, body) in [
        ("full_name", json!({"full_name": "   "})),
        ("email", json!({"full_name": "И", "email": "not-an-email"})),
        ("phone", json!({"full_name": "И", "phone": "12345"})),
        ("telegram", json!({"full_name": "И", "telegram": "a"})),
        ("whatsapp", json!({"full_name": "И", "whatsapp": "zz"})),
        (
            "expected_salary",
            json!({"full_name": "И", "expected_salary": -5}),
        ),
    ] {
        let (s, err) = create(&app, &token, body).await;
        assert_eq!(s, 400, "{field}: {err}");
        assert_eq!(err["field"], field);
    }
    assert_eq!(app.count(tenant).await, 0);
    app.close().await;
}

#[tokio::test]
async fn creation_requires_authentication() {
    let app = setup().await;
    let (s, _) = app
        .send("POST", "/persons", None, Some(json!({"full_name": "И"})))
        .await;
    assert_eq!(s, 401);
    app.close().await;
}

// ---------- предупреждение о дубле ----------

async fn seed_ivan(app: &App, token: &str) -> String {
    let (s, p) = create(
        app,
        token,
        json!({"full_name": "Иван Петров", "email": "ivan@mail.ru", "phone": "+7 916 123 45 67"}),
    )
    .await;
    assert_eq!(s, 201);
    p["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn duplicate_by_email_is_a_409_with_the_existing_person() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let ivan = seed_ivan(&app, &token).await;

    let (s, err) = create(
        &app,
        &token,
        json!({"full_name": "Другой", "email": "IVAN@mail.ru"}),
    )
    .await;
    assert_eq!(s, 409, "{err}");
    assert_eq!(err["duplicate"]["id"], ivan);
    assert_eq!(err["duplicate"]["full_name"], "Иван Петров");
    assert_eq!(err["duplicate"]["matched_by"], "email");
    assert_eq!(app.count(tenant).await, 1, "nothing was created");
    app.close().await;
}

#[tokio::test]
async fn duplicate_by_phone_and_by_name() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    seed_ivan(&app, &token).await;

    let (s, err) = create(
        &app,
        &token,
        json!({"full_name": "Кто-то", "phone": "8 916 123-45-67"}),
    )
    .await;
    assert_eq!(
        (s, err["duplicate"]["matched_by"].as_str()),
        (409, Some("phone"))
    );

    let (s, err) = create(&app, &token, json!({"full_name": "  иван   ПЕТРОВ "})).await;
    assert_eq!(
        (s, err["duplicate"]["matched_by"].as_str()),
        (409, Some("name"))
    );
    app.close().await;
}

#[tokio::test]
async fn email_match_wins_over_phone_and_name() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let ivan = seed_ivan(&app, &token).await; // email + телефон
    // другой человек: совпадает только ФИО с будущей заявкой
    let (s, other) = create(
        &app,
        &token,
        json!({"full_name": "Пётр Сидоров", "phone": "+7 999 000 11 22"}),
    )
    .await;
    assert_eq!(s, 201);
    assert_ne!(other["id"], ivan);

    // заявка: email Ивана, телефон Петра, имя Петра -> побеждает email
    let (s, err) = create(
        &app,
        &token,
        json!({"full_name": "Пётр Сидоров", "email": "ivan@mail.ru", "phone": "+7 999 000 11 22"}),
    )
    .await;
    assert_eq!(s, 409);
    assert_eq!(err["duplicate"]["id"], ivan);
    assert_eq!(err["duplicate"]["matched_by"], "email");
    app.close().await;
}

#[tokio::test]
async fn confirmed_creation_goes_through() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    seed_ivan(&app, &token).await;

    let (s, _) = create(&app, &token, json!({"full_name": "Иван Петров"})).await;
    assert_eq!(s, 409);
    let (s, p) = create(
        &app,
        &token,
        json!({"full_name": "Иван Петров", "confirm_duplicate": true}),
    )
    .await;
    assert_eq!(s, 201, "{p}");
    assert_eq!(app.count(tenant).await, 2);
    app.close().await;
}

#[tokio::test]
async fn another_tenant_is_never_a_duplicate() {
    let app = setup().await;
    let (a, b) = (TenantId::new(), TenantId::new());
    let token_a = app.token(a, &[Role::Recruiter]);
    let token_b = app.token(b, &[Role::Recruiter]);
    seed_ivan(&app, &token_a).await;

    let (s, _) = create(
        &app,
        &token_b,
        json!({"full_name": "Иван Петров", "email": "ivan@mail.ru", "phone": "+7 916 123 45 67"}),
    )
    .await;
    assert_eq!(
        s, 201,
        "same data in another organization is a different person"
    );
    assert_eq!((app.count(a).await, app.count(b).await), (1, 1));
    app.close().await;
}
