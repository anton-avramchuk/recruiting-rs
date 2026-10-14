mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};

async fn create(app: &App, token: &str, body: Value) -> Value {
    let (s, p) = app.send("POST", "/persons", Some(token), Some(body)).await;
    assert_eq!(s, 201, "{p}");
    p
}

async fn put(app: &App, token: &str, id: &Value, what: &str, body: Value) -> (u16, Value) {
    app.send(
        "PUT",
        &format!("/persons/{}/{what}", id.as_str().unwrap()),
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

fn job(company: &str, start: &str, end: Option<&str>) -> Value {
    json!({"company": company, "position": "Разработчик", "start": start, "end": end})
}

async fn fixture() -> (App, String, Value) {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let p = create(&app, &token, json!({"full_name": "Иван Петров"})).await;
    (app, token, p)
}

// ---------- места работы и стаж ----------

#[tokio::test]
async fn employments_are_replaced_as_a_whole_and_keep_their_order() {
    let (app, token, p) = fixture().await;
    let (s, updated) = put(
        &app,
        &token,
        &p["id"],
        "employments",
        json!([
            job("Beta", "2023-01-01", Some("2024-09-01")),
            job("Alpha", "2020-01-01", Some("2022-08-01"))
        ]),
    )
    .await;
    assert_eq!(s, 200, "{updated}");
    let companies: Vec<_> = updated["employments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["company"].clone())
        .collect();
    assert_eq!(companies, [json!("Beta"), json!("Alpha")]);
    assert_eq!(updated["employments"][1]["end"], "2022-08-01");
    assert_eq!(card(&app, &token, &p["id"]).await, updated);

    // повторная замена полностью вытесняет прежний список
    let (_, again) = put(
        &app,
        &token,
        &p["id"],
        "employments",
        json!([job("Gamma", "2024-01-01", None)]),
    )
    .await;
    assert_eq!(again["employments"].as_array().unwrap().len(), 1);
    app.close().await;
}

#[tokio::test]
async fn experience_comes_from_the_history_and_drives_the_list_filter() {
    let (app, token, p) = fixture().await;
    // 2 г. 7 мес. + 1 г. 8 мес. = 4 г. 3 мес.
    let (_, updated) = put(
        &app,
        &token,
        &p["id"],
        "employments",
        json!([
            job("A", "2020-01-01", Some("2022-08-01")),
            job("B", "2023-01-01", Some("2024-09-01"))
        ]),
    )
    .await;
    assert_eq!(updated["experience_years"], 4);

    let (_, page) = app
        .send("GET", "/persons?min_experience=4", Some(&token), None)
        .await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["experience_years"], 4);
    let (_, page) = app
        .send("GET", "/persons?min_experience=5", Some(&token), None)
        .await;
    assert_eq!(page["total"], 0);

    let stored: i32 = sqlx::query_scalar("SELECT experience_years FROM persons")
        .fetch_one(&app.db.owner)
        .await
        .unwrap();
    assert_eq!(stored, 4, "stored value matches the history");
    app.close().await;
}

#[tokio::test]
async fn overlapping_periods_count_once_and_the_current_job_runs_until_today() {
    let (app, token, p) = fixture().await;
    let (_, overlap) = put(
        &app,
        &token,
        &p["id"],
        "employments",
        json!([
            job("A", "2020-01-01", Some("2022-01-01")),
            job("B", "2021-01-01", Some("2023-01-01"))
        ]),
    )
    .await;
    assert_eq!(overlap["experience_years"], 3);

    // «сегодня» в тестах - 2026-01-01: с 2020-03-01 это 5 лет 10 месяцев
    let (_, current) = put(
        &app,
        &token,
        &p["id"],
        "employments",
        json!([job("Now", "2020-03-01", None)]),
    )
    .await;
    assert_eq!(current["experience_years"], 5);
    app.close().await;
}

#[tokio::test]
async fn declared_experience_applies_only_without_history() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let p = create(
        &app,
        &token,
        json!({"full_name": "Иван", "declared_experience_years": 5}),
    )
    .await;
    assert_eq!(p["experience_years"], 5);

    let (_, with_history) = put(
        &app,
        &token,
        &p["id"],
        "employments",
        json!([job("A", "2024-01-01", Some("2025-01-01"))]),
    )
    .await;
    assert_eq!(with_history["experience_years"], 1);

    let (_, cleared) = put(&app, &token, &p["id"], "employments", json!([])).await;
    assert_eq!(
        cleared["experience_years"], 5,
        "falls back to the declared value"
    );
    assert_eq!(cleared["employments"], json!([]));
    app.close().await;
}

#[tokio::test]
async fn invalid_employments_are_a_400_and_keep_the_previous_list() {
    let (app, token, p) = fixture().await;
    put(
        &app,
        &token,
        &p["id"],
        "employments",
        json!([job("Keep", "2020-01-01", Some("2021-01-01"))]),
    )
    .await;
    let before = card(&app, &token, &p["id"]).await;

    for (field, body) in [
        (
            "employments",
            json!([job("A", "2021-01-01", Some("2020-01-01"))]),
        ),
        ("employments", json!([job("", "2020-01-01", None)])),
        ("start", json!([job("A", "01.01.2020", None)])),
        ("end", json!([job("A", "2020-01-01", Some("вчера"))])),
        ("start", json!([job("A", "2020-02-30", None)])),
    ] {
        let (s, err) = put(&app, &token, &p["id"], "employments", body).await;
        assert_eq!(s, 400, "{field}: {err}");
        assert_eq!(err["field"], field);
    }
    // один плохой элемент отклоняет весь список
    let (s, _) = put(
        &app,
        &token,
        &p["id"],
        "employments",
        json!([
            job("Good", "2020-01-01", None),
            job("Bad", "2022-01-01", Some("2021-01-01"))
        ]),
    )
    .await;
    assert_eq!(s, 400);
    assert_eq!(card(&app, &token, &p["id"]).await, before);
    app.close().await;
}

// ---------- образование ----------

#[tokio::test]
async fn educations_are_replaced_and_validated() {
    let (app, token, p) = fixture().await;
    let (s, updated) = put(
        &app,
        &token,
        &p["id"],
        "educations",
        json!([
            {"institution": "МГУ", "specialty": "Прикладная математика", "year_from": 2010, "year_to": 2015},
            {"institution": "Coursera"}
        ]),
    )
    .await;
    assert_eq!(s, 200, "{updated}");
    assert_eq!(updated["educations"][0]["institution"], "МГУ");
    assert_eq!(updated["educations"][1]["year_to"], Value::Null);
    assert_eq!(card(&app, &token, &p["id"]).await, updated);

    for body in [
        json!([{"institution": "МГУ", "year_from": 2015, "year_to": 2010}]),
        json!([{"institution": "  "}]),
    ] {
        let (s, err) = put(&app, &token, &p["id"], "educations", body).await;
        assert_eq!(s, 400);
        assert_eq!(err["field"], "educations");
    }
    assert_eq!(
        card(&app, &token, &p["id"]).await,
        updated,
        "unchanged after rejected updates"
    );

    let (_, cleared) = put(&app, &token, &p["id"], "educations", json!([])).await;
    assert_eq!(cleared["educations"], json!([]));
    // история образования не влияет на стаж
    assert_eq!(cleared["experience_years"], 0);
    app.close().await;
}

// ---------- доступ ----------

#[tokio::test]
async fn history_is_confined_to_the_organization_and_needs_write_access() {
    let (app, token, p) = fixture().await;
    let other = app.token(TenantId::new(), &[Role::Owner]);
    let (s, _) = put(
        &app,
        &other,
        &p["id"],
        "employments",
        json!([job("A", "2020-01-01", None)]),
    )
    .await;
    assert_eq!(s, 404);

    let viewer = app.token(TenantId::new(), &[Role::Viewer]);
    let (s, _) = put(&app, &viewer, &p["id"], "educations", json!([])).await;
    assert_eq!(s, 403);

    let unknown = json!(uuid::Uuid::new_v4().to_string());
    let (s, _) = put(&app, &token, &unknown, "employments", json!([])).await;
    assert_eq!(s, 404);
    app.close().await;
}
