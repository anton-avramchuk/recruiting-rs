//! Сервис кандидатов в составе платформы: процесс поднят, токен identity принят, BFF проксирует.
use e2e::Stack;
use reqwest::Client;
use serde_json::{Value, json};

#[tokio::test]
async fn candidates_are_reachable_through_the_bff_with_an_identity_token() {
    let stack = Stack::start().await;
    stack
        .seed_tenant("acme", "boss@acme.io", "boss-password-1")
        .await;

    let http = Client::builder().cookie_store(true).build().unwrap();
    let login: Value = http
        .post(format!("{}/bff/login", stack.bff_crm_url))
        .header("x-csrf-token", "login")
        .json(&json!({"tenant": "acme", "email": "boss@acme.io", "password": "boss-password-1"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let csrf = login["csrf_token"].as_str().unwrap().to_string();

    // пустой список организации
    let page: Value = http
        .get(format!("{}/api/persons", stack.bff_crm_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page["total"], 0);

    // создание через BFF с CSRF-токеном
    let res = http
        .post(format!("{}/api/persons", stack.bff_crm_url))
        .header("x-csrf-token", &csrf)
        .json(&json!({"full_name": "Иван Петров", "email": "ivan@mail.ru"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);

    let page: Value = http
        .get(format!("{}/api/persons?q=иван", stack.bff_crm_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page["total"], 1);

    // сам сервис не принимает запросы без токена, служебные маршруты BFF не публикует
    let direct = http
        .get(format!("{}/persons", stack.candidate_url))
        .send()
        .await
        .unwrap();
    assert_eq!(direct.status(), 401);
    let hidden = http
        .get(format!("{}/api/openapi.json", stack.bff_crm_url))
        .send()
        .await
        .unwrap();
    assert_eq!(hidden.status(), 404);

    stack.stop().await;
}
