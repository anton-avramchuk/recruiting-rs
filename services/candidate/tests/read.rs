mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};

async fn add(app: &App, token: &str, body: Value) -> String {
    let mut body = body;
    body["confirm_duplicate"] = json!(true);
    let (s, p) = app.send("POST", "/persons", Some(token), Some(body)).await;
    assert_eq!(s, 201, "{p}");
    p["id"].as_str().unwrap().to_string()
}

async fn list(app: &App, token: &str, query: &str) -> Value {
    let (s, body) = app
        .send("GET", &format!("/persons{query}"), Some(token), None)
        .await;
    assert_eq!(s, 200, "{query}: {body}");
    body
}

fn names(page: &Value) -> Vec<&str> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["full_name"].as_str().unwrap())
        .collect()
}

/// Пять кандидатов с разными свойствами; создаются по порядку.
async fn fixture() -> (App, String) {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    add(&app, &token, json!({
        "full_name": "Анна Смирнова", "email": "anna@mail.ru", "phone": "+7 916 123 45 67",
        "source": "hh.ru", "expected_salary": 300000, "skills": ["Rust", "SQL"], "declared_experience_years": 6
    })).await;
    add(&app, &token, json!({
        "full_name": "Борис Иванов", "email": "boris@example.com", "phone": "+7 903 555 66 77",
        "source": "Реферал", "expected_salary": 200000, "skills": ["Go"], "declared_experience_years": 2
    })).await;
    add(&app, &token, json!({
        "full_name": "Виктор Петров", "source": "hh.ru", "skills": ["rust", "Docker"], "declared_experience_years": 4,
        "summary": "Нужен 100% удалёнка"
    })).await;
    add(
        &app,
        &token,
        json!({"full_name": "Галина Орлова", "expected_salary": 150000}),
    )
    .await;
    add(
        &app,
        &token,
        json!({"full_name": "Дмитрий Орлов", "skills": ["SQL"], "declared_experience_years": 10}),
    )
    .await;
    (app, token)
}

// ---------- карточка ----------

#[tokio::test]
async fn card_returns_the_person() {
    let (app, token) = fixture().await;
    let page = list(&app, &token, "?q=anna@").await;
    let id = page["items"][0]["id"].as_str().unwrap().to_string();

    let (s, p) = app
        .send("GET", &format!("/persons/{id}"), Some(&token), None)
        .await;
    assert_eq!(s, 200);
    assert_eq!(p["id"], id);
    assert_eq!(p["full_name"], "Анна Смирнова");
    assert_eq!(p["phone"], "+79161234567");
    assert_eq!(p["skills"], json!(["Rust", "SQL"]));
    assert_eq!(p["experience_years"], 6);
    assert_eq!(p["employments"], json!([]));
    app.close().await;
}

#[tokio::test]
async fn unknown_and_foreign_persons_are_404() {
    let (app, token) = fixture().await;
    let id = list(&app, &token, "").await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (s, _) = app
        .send(
            "GET",
            &format!("/persons/{}", uuid::Uuid::new_v4()),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(s, 404);

    let other = app.token(TenantId::new(), &[Role::Owner]);
    let (s, _) = app
        .send("GET", &format!("/persons/{id}"), Some(&other), None)
        .await;
    assert_eq!(s, 404, "another organization cannot see the card");
    app.close().await;
}

// ---------- список ----------

#[tokio::test]
async fn list_is_newest_first_with_defaults() {
    let (app, token) = fixture().await;
    let page = list(&app, &token, "").await;
    assert_eq!(
        names(&page),
        [
            "Дмитрий Орлов",
            "Галина Орлова",
            "Виктор Петров",
            "Борис Иванов",
            "Анна Смирнова"
        ]
    );
    assert_eq!(
        (
            page["total"].as_i64(),
            page["page"].as_i64(),
            page["page_size"].as_i64()
        ),
        (Some(5), Some(1), Some(20))
    );
    let first = &page["items"][0];
    assert!(
        first.get("employments").is_none(),
        "summary rows carry no history"
    );
    assert_eq!(first["experience_years"], 10);
    app.close().await;
}

#[tokio::test]
async fn search_matches_name_email_phone_digits_and_skills_ignoring_case() {
    let (app, token) = fixture().await;
    assert_eq!(
        names(&list(&app, &token, "?q=ОРЛОВ").await),
        ["Дмитрий Орлов", "Галина Орлова"]
    );
    assert_eq!(
        names(&list(&app, &token, "?q=example.COM").await),
        ["Борис Иванов"]
    );
    // цифры телефона находятся независимо от написания
    assert_eq!(
        names(&list(&app, &token, "?q=123-45").await),
        ["Анна Смирнова"]
    );
    assert_eq!(
        names(&list(&app, &token, "?q=555%2066").await),
        ["Борис Иванов"]
    );
    // навык
    assert_eq!(
        names(&list(&app, &token, "?q=docker").await),
        ["Виктор Петров"]
    );
    assert!(names(&list(&app, &token, "?q=zzz-nothing").await).is_empty());
    app.close().await;
}

#[tokio::test]
async fn search_special_characters_are_literal() {
    let (app, token) = fixture().await;
    // «100%» есть только в выжимке Виктора, а поиск идёт по ФИО/email/телефону/навыкам
    assert!(names(&list(&app, &token, "?q=100%25").await).is_empty());
    assert!(
        names(&list(&app, &token, "?q=%25").await).is_empty(),
        "% is not a wildcard"
    );
    assert!(
        names(&list(&app, &token, "?q=_").await).is_empty(),
        "_ is not a wildcard"
    );
    app.close().await;
}

#[tokio::test]
async fn filters_by_source_skill_experience_and_salary() {
    let (app, token) = fixture().await;
    assert_eq!(
        names(&list(&app, &token, "?source=HH.ru").await),
        ["Виктор Петров", "Анна Смирнова"]
    );
    assert_eq!(
        names(&list(&app, &token, "?skill=rust").await),
        ["Виктор Петров", "Анна Смирнова"]
    );
    assert_eq!(
        names(&list(&app, &token, "?min_experience=4&max_experience=6").await),
        ["Виктор Петров", "Анна Смирнова"]
    );
    assert_eq!(
        names(&list(&app, &token, "?min_salary=200000").await),
        ["Борис Иванов", "Анна Смирнова"]
    );
    assert_eq!(
        names(&list(&app, &token, "?max_salary=150000").await),
        ["Галина Орлова"]
    );
    app.close().await;
}

#[tokio::test]
async fn filters_combine_with_and() {
    let (app, token) = fixture().await;
    assert_eq!(
        names(&list(&app, &token, "?skill=rust&min_experience=5").await),
        ["Анна Смирнова"]
    );
    assert_eq!(
        names(&list(&app, &token, "?q=орлов&skill=sql").await),
        ["Дмитрий Орлов"]
    );
    assert!(names(&list(&app, &token, "?skill=rust&source=Реферал").await).is_empty());
    app.close().await;
}

#[tokio::test]
async fn pagination_reports_total_and_slices_the_list() {
    let (app, token) = fixture().await;
    let p1 = list(&app, &token, "?page=1&page_size=3").await;
    assert_eq!(
        names(&p1),
        ["Дмитрий Орлов", "Галина Орлова", "Виктор Петров"]
    );
    assert_eq!(p1["total"], 5);
    let p2 = list(&app, &token, "?page=2&page_size=3").await;
    assert_eq!(names(&p2), ["Борис Иванов", "Анна Смирнова"]);
    assert!(names(&list(&app, &token, "?page=9&page_size=3").await).is_empty());
    // total считается с учётом фильтров
    let filtered = list(&app, &token, "?skill=sql&page_size=1").await;
    assert_eq!(
        (names(&filtered), filtered["total"].as_i64()),
        (vec!["Дмитрий Орлов"], Some(2))
    );
    app.close().await;
}

#[tokio::test]
async fn page_size_is_clamped_and_bad_paging_is_rejected() {
    let (app, token) = fixture().await;
    assert_eq!(
        list(&app, &token, "?page_size=1000").await["page_size"],
        100
    );
    for bad in ["?page=0", "?page_size=0", "?page=-1", "?page_size=-5"] {
        let (s, _) = app
            .send("GET", &format!("/persons{bad}"), Some(&token), None)
            .await;
        assert_eq!(s, 400, "{bad}");
    }
    app.close().await;
}

#[tokio::test]
async fn lists_never_cross_organizations() {
    let (app, token) = fixture().await;
    let other = app.token(TenantId::new(), &[Role::Recruiter]);
    add(
        &app,
        &other,
        json!({"full_name": "Чужой Человек", "skills": ["Rust"]}),
    )
    .await;

    assert_eq!(list(&app, &other, "").await["total"], 1);
    assert_eq!(list(&app, &token, "").await["total"], 5);
    assert_eq!(
        names(&list(&app, &other, "?skill=rust").await),
        ["Чужой Человек"]
    );
    app.close().await;
}

#[tokio::test]
async fn viewers_can_read() {
    let (app, token) = fixture().await;
    let viewer = app.token(TenantId::new(), &[Role::Viewer]);
    assert_eq!(list(&app, &viewer, "").await["total"], 0);
    let _ = token;
    app.close().await;
}
