//! Кандидаты на настоящей платформе (процессы + bff-crm): роли, дубли, изоляция организаций, приостановка.
use std::time::Duration;

use e2e::Stack;
use serde_json::{Value, json};

const BOSS: (&str, &str) = ("boss@acme.io", "boss-password-1");
const RECRUITER: (&str, &str) = ("rec@acme.io", "recruiter-pass-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-pass-1");
const GLOBEX_BOSS: (&str, &str) = ("boss@globex.io", "globex-password-1");

async fn json_of(res: reqwest::Response) -> Value {
    res.json().await.unwrap_or(Value::Null)
}

#[tokio::test]
async fn candidates_across_organizations_roles_and_suspension() {
    let stack = Stack::start().await;
    stack.seed_tenant("acme", BOSS.0, BOSS.1).await;
    stack
        .seed_tenant("globex", GLOBEX_BOSS.0, GLOBEX_BOSS.1)
        .await;

    let boss = stack.crm_client("acme", BOSS.0, BOSS.1).await;
    stack
        .add_user(&boss, RECRUITER.0, RECRUITER.1, "recruiter")
        .await;
    stack.add_user(&boss, VIEWER.0, VIEWER.1, "viewer").await;
    let recruiter = stack.crm_client("acme", RECRUITER.0, RECRUITER.1).await;
    let viewer = stack.crm_client("acme", VIEWER.0, VIEWER.1).await;
    let globex = stack
        .crm_client("globex", GLOBEX_BOSS.0, GLOBEX_BOSS.1)
        .await;

    // рекрутер создаёт кандидата; повтор по email - предупреждение, подтверждение - второй кандидат
    let ivan =
        json!({"full_name": "Иван Петров", "email": "ivan@mail.ru", "phone": "8 916 123-45-67"});
    let res = recruiter.post("/api/persons", ivan.clone()).await;
    assert_eq!(res.status(), 201);
    let ivan_id = json_of(res).await["id"].as_str().unwrap().to_string();

    let res = recruiter
        .post(
            "/api/persons",
            json!({"full_name": "Иван П.", "email": "IVAN@mail.ru"}),
        )
        .await;
    assert_eq!(res.status(), 409);
    let dup = json_of(res).await;
    assert_eq!(dup["duplicate"]["id"], ivan_id);
    assert_eq!(dup["duplicate"]["matched_by"], "email");

    let res = recruiter
        .post(
            "/api/persons",
            json!({"full_name": "Иван П.", "email": "ivan@mail.ru", "confirm_duplicate": true}),
        )
        .await;
    assert_eq!(res.status(), 201);

    // история и стаж
    let res = recruiter
        .put(
            &format!("/api/persons/{ivan_id}/employments"),
            json!([{"company": "Acme", "position": "Dev", "start": "2020-01-01", "end": "2022-08-01"},
                   {"company": "Beta", "position": "Lead", "start": "2023-01-01", "end": "2024-09-01"}]),
        )
        .await;
    assert_eq!(res.status(), 200);
    assert_eq!(json_of(res).await["experience_years"], 4);

    // viewer читает и ищет, но не пишет
    let page = json_of(viewer.get("/api/persons?q=ivan%40mail").await).await;
    assert_eq!(page["total"], 2);
    let res = viewer
        .post("/api/persons", json!({"full_name": "Тайный Гость"}))
        .await;
    assert_eq!(res.status(), 403);

    // удаляет только владелец
    assert_eq!(
        recruiter
            .delete(&format!("/api/persons/{ivan_id}"))
            .await
            .status(),
        403
    );

    // другая организация не видит чужих и не считает совпадение дублем
    assert_eq!(json_of(globex.get("/api/persons").await).await["total"], 0);
    assert_eq!(
        globex
            .get(&format!("/api/persons/{ivan_id}"))
            .await
            .status(),
        404
    );
    assert_eq!(
        globex
            .delete(&format!("/api/persons/{ivan_id}"))
            .await
            .status(),
        404
    );
    let res = globex.post("/api/persons", ivan).await;
    assert_eq!(
        res.status(),
        201,
        "the same person in another organization is not a duplicate"
    );

    assert_eq!(
        boss.delete(&format!("/api/persons/{ivan_id}"))
            .await
            .status(),
        204
    );
    assert_eq!(json_of(boss.get("/api/persons").await).await["total"], 1);

    // сервис не принимает запросы без токена, служебные маршруты BFF не публикует
    let anonymous = reqwest::Client::new();
    let direct = anonymous
        .get(format!("{}/persons", stack.candidate_url))
        .send()
        .await
        .unwrap();
    assert_eq!(direct.status(), 401);
    let hidden = recruiter.get("/api/openapi.json").await;
    assert_eq!(hidden.status(), 404);

    // приостановка организации закрывает доступ к её кандидатам (access-токен живёт 5 с,
    // обновление отвечает отказом, сессия завершается)
    stack.set_tenant_status("acme", "suspend").await;
    tokio::time::sleep(Duration::from_millis(1500)).await; // TTL кэша статуса в identity
    assert_eq!(recruiter.get("/api/persons").await.status(), 401);
    let blocked = reqwest::Client::new()
        .post(format!("{}/bff/login", stack.bff_crm_url))
        .header("x-csrf-token", "login")
        .json(&json!({"tenant": "acme", "email": RECRUITER.0, "password": RECRUITER.1}))
        .send()
        .await
        .unwrap();
    assert_eq!(blocked.status(), 403);
    // другая организация продолжает работать
    assert_eq!(globex.get("/api/persons").await.status(), 200);

    // возобновление возвращает доступ, данные на месте
    stack.set_tenant_status("acme", "activate").await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let boss_again = stack.crm_client("acme", BOSS.0, BOSS.1).await;
    assert_eq!(
        json_of(boss_again.get("/api/persons").await).await["total"],
        1
    );

    stack.stop().await;
}
