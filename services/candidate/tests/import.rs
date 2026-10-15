mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};

async fn import(app: &App, token: &str, rows: Value, source: Option<&str>) -> (u16, Value) {
    let mut body = json!({"rows": rows});
    if let Some(s) = source {
        body["default_source"] = json!(s);
    }
    app.send("POST", "/persons/import", Some(token), Some(body))
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

fn outcomes(res: &Value) -> Vec<&str> {
    res["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["outcome"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn new_people_are_created() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let (s, res) = import(
        &app,
        &token,
        json!([
            {"full_name": "Пётр Гусев", "email": "petr@mail.ru", "city": "Москва", "expected_salary": 280000, "skills": ["Go", "K8s"]},
            {"full_name": "Мария Орлова", "phone": "+7 911 555-44-33"}
        ]),
        Some("Импорт из файла"),
    )
    .await;
    assert_eq!(s, 200, "{res}");
    assert_eq!(outcomes(&res), ["created", "created"]);
    assert_eq!(
        (
            res["created"].as_i64(),
            res["merged"].as_i64(),
            res["rejected"].as_i64()
        ),
        (Some(2), Some(0), Some(0))
    );
    assert_eq!(res["results"][1]["row"], 1);
    let first = card(&app, &token, &res["results"][0]["id"]).await;
    assert_eq!(first["full_name"], "Пётр Гусев");
    assert_eq!(first["city"], "Москва");
    assert_eq!(first["skills"], json!(["Go", "K8s"]));
    assert_eq!(first["source"], "Импорт из файла", "default source applied");
    app.close().await;
}

#[tokio::test]
async fn an_existing_person_is_filled_in_not_duplicated() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let (_, existing) = app
        .send(
            "POST",
            "/persons",
            Some(&token),
            Some(json!({"full_name": "Иван Петров", "email": "ivan@mail.ru", "phone": "+7 903 000 00 00", "source": "Реферал"})),
        )
        .await;
    let (s, res) = import(
        &app,
        &token,
        json!([{
            "full_name": "Иван Петров",
            "email": "IVAN@mail.ru",
            "phone": "+7 916 123 45 67",
            "city": "Казань",
            "source": "hh.ru",
            "skills": ["Rust"]
        }]),
        None,
    )
    .await;
    assert_eq!(s, 200, "{res}");
    assert_eq!(outcomes(&res), ["merged"]);
    assert_eq!(res["results"][0]["id"], existing["id"]);
    let after = card(&app, &token, &existing["id"]).await;
    assert_eq!(after["city"], "Казань", "empty field filled");
    assert_eq!(after["phone"], "+79030000000", "existing phone kept");
    assert_eq!(after["source"], "Реферал", "existing source kept");
    assert_eq!(after["skills"], json!(["Rust"]));
    assert_eq!(app.count(app_tenant(&token, &app).await).await, 1);
    app.close().await;
}

async fn app_tenant(_token: &str, app: &App) -> TenantId {
    // единственная организация теста: берём её из базы
    let id: uuid::Uuid = sqlx::query_scalar("SELECT tenant_id FROM persons LIMIT 1")
        .fetch_one(&app.db.owner)
        .await
        .unwrap();
    TenantId(id)
}

#[tokio::test]
async fn people_are_matched_by_phone_and_by_name() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let (_, by_phone) = app
        .send(
            "POST",
            "/persons",
            Some(&token),
            Some(json!({"full_name": "Анна К", "phone": "8 (916) 123-45-67"})),
        )
        .await;
    let (_, by_name) = app
        .send(
            "POST",
            "/persons",
            Some(&token),
            Some(json!({"full_name": "Борис Иванов"})),
        )
        .await;
    let (_, res) = import(
        &app,
        &token,
        json!([
            {"full_name": "Совсем другое имя", "phone": "+7 916 123 45 67", "city": "Уфа"},
            {"full_name": "борис   иванов", "city": "Омск"}
        ]),
        None,
    )
    .await;
    assert_eq!(outcomes(&res), ["merged", "merged"]);
    assert_eq!(res["results"][0]["id"], by_phone["id"]);
    assert_eq!(res["results"][1]["id"], by_name["id"]);
    assert_eq!(
        card(&app, &token, &by_phone["id"]).await["full_name"],
        "Анна К"
    );
    app.close().await;
}

#[tokio::test]
async fn bad_rows_are_rejected_and_the_rest_are_imported() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let (s, res) = import(
        &app,
        &token,
        json!([
            {"full_name": "Анна Первая"},
            {"full_name": "   "},
            {"full_name": "Борис Второй", "email": "не email"},
            {"full_name": "Вера Третья", "expected_salary": -5},
            {"full_name": "Глеб Четвёртый", "employments": [{"company": "", "position": "Dev", "start": "2020-01-01"}]},
            {"full_name": "Дарья Пятая"}
        ]),
        None,
    )
    .await;
    assert_eq!(s, 200, "{res}");
    assert_eq!(
        outcomes(&res),
        [
            "created", "rejected", "rejected", "rejected", "rejected", "created"
        ]
    );
    assert_eq!(res["rejected"], 4);
    assert_eq!(res["results"][1]["field"], "full_name");
    assert_eq!(res["results"][2]["field"], "email");
    assert_eq!(res["results"][3]["field"], "expected_salary");
    assert_eq!(res["results"][4]["field"], "employments");
    assert!(res["results"][1]["message"].as_str().unwrap().len() > 3);
    let (_, list) = app.send("GET", "/persons", Some(&token), None).await;
    assert_eq!(list["total"], 2, "rejected rows leave nothing behind");
    app.close().await;
}

#[tokio::test]
async fn the_same_person_twice_in_one_batch_is_merged() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let (_, res) = import(
        &app,
        &token,
        json!([
            {"full_name": "Иван Петров", "email": "ivan@mail.ru"},
            {"full_name": "И. Петров", "email": "ivan@mail.ru", "city": "Омск"}
        ]),
        None,
    )
    .await;
    assert_eq!(outcomes(&res), ["created", "merged"]);
    assert_eq!(res["results"][0]["id"], res["results"][1]["id"]);
    assert_eq!(
        card(&app, &token, &res["results"][0]["id"]).await["city"],
        "Омск"
    );
    app.close().await;
}

#[tokio::test]
async fn importing_the_same_file_again_creates_nothing() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let rows = json!([
        {"full_name": "Пётр Гусев", "email": "petr@mail.ru", "skills": ["Go"],
         "employments": [{"company": "Яндекс", "position": "Разработчик", "start": "2021-01-01"}],
         "educations": [{"institution": "МФТИ", "year_to": 2019}]},
        {"full_name": "Мария Орлова", "phone": "+7 911 555-44-33"}
    ]);
    let (_, first) = import(&app, &token, rows.clone(), None).await;
    assert_eq!(first["created"], 2);
    let (s, second) = import(&app, &token, rows, None).await;
    assert_eq!(s, 200);
    assert_eq!(
        (second["created"].as_i64(), second["merged"].as_i64()),
        (Some(0), Some(2))
    );
    assert_eq!(app.count(tenant).await, 2);
    let petr = card(&app, &token, &first["results"][0]["id"]).await;
    assert_eq!(
        petr["employments"].as_array().unwrap().len(),
        1,
        "history is not repeated"
    );
    assert_eq!(petr["educations"].as_array().unwrap().len(), 1);
    app.close().await;
}

#[tokio::test]
async fn history_is_saved_and_counts_towards_experience() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let (_, res) = import(
        &app,
        &token,
        json!([{
            "full_name": "Ирина Соколова",
            "declared_experience_years": 1,
            "employments": [
                {"company": "Ozon", "position": "Аналитик", "start": "2022-01-01"},
                {"company": "X5", "position": "Аналитик", "start": "2019-01-01", "end": "2022-12-31"}
            ],
            "educations": [{"institution": "КФУ", "specialty": "Прикладная математика", "year_to": 2017}]
        }]),
        None,
    )
    .await;
    let p = card(&app, &token, &res["results"][0]["id"]).await;
    assert_eq!(p["employments"].as_array().unwrap().len(), 2);
    assert_eq!(p["educations"][0]["institution"], "КФУ");
    assert!(p["experience_years"].as_i64().unwrap() >= 4, "{p}");
    app.close().await;
}

#[tokio::test]
async fn batch_size_is_limited() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let rows: Vec<Value> = (0..501)
        .map(|n| json!({"full_name": format!("Человек {n}")}))
        .collect();
    let (s, _) = import(&app, &token, json!(rows), None).await;
    assert_eq!(s, 400);
    let (_, list) = app.send("GET", "/persons", Some(&token), None).await;
    assert_eq!(list["total"], 0);
    let (s, empty) = import(&app, &token, json!([]), None).await;
    assert_eq!(s, 200);
    assert_eq!(empty["results"].as_array().unwrap().len(), 0);
    app.close().await;
}

#[tokio::test]
async fn viewers_and_strangers_cannot_import() {
    let app = setup().await;
    let tenant = TenantId::new();
    let viewer = app.token(tenant, &[Role::Viewer]);
    let (s, _) = import(&app, &viewer, json!([{"full_name": "Анна"}]), None).await;
    assert_eq!(s, 403);
    let (s, _) = app
        .send("POST", "/persons/import", None, Some(json!({"rows": []})))
        .await;
    assert_eq!(s, 401);
    assert_eq!(app.count(tenant).await, 0);
    app.close().await;
}

#[tokio::test]
async fn matches_are_looked_up_in_the_own_organization_only() {
    let app = setup().await;
    let (a, b) = (TenantId::new(), TenantId::new());
    let (ta, tb) = (app.token(a, &[Role::Owner]), app.token(b, &[Role::Owner]));
    let (_, first) = import(
        &app,
        &ta,
        json!([{"full_name": "Иван Петров", "email": "ivan@mail.ru"}]),
        None,
    )
    .await;
    let (_, second) = import(
        &app,
        &tb,
        json!([{"full_name": "Иван Петров", "email": "ivan@mail.ru", "city": "Омск"}]),
        None,
    )
    .await;
    assert_eq!(
        outcomes(&second),
        ["created"],
        "another organization is not a match"
    );
    assert_ne!(first["results"][0]["id"], second["results"][0]["id"]);
    assert_eq!(
        card(&app, &ta, &first["results"][0]["id"]).await["city"],
        Value::Null
    );
    app.close().await;
}

#[tokio::test]
async fn imported_people_reach_the_change_feed() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    import(
        &app,
        &token,
        json!([{"full_name": "Анна", "city": "Омск"}]),
        None,
    )
    .await;
    let (s, feed) = app
        .send(
            "GET",
            &format!("/internal/tenants/{}/person-changes?after=0", tenant.0),
            Some(common::SERVICE_TOKEN),
            None,
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(feed["items"].as_array().unwrap().len(), 1);
    assert_eq!(feed["items"][0]["snapshot"]["city"], "Омск");
    app.close().await;
}
