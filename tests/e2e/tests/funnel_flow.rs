//! Воронка на настоящей платформе (процессы + bff-crm): кандидат → вакансия → отклик → этапы,
//! видимость правок кандидата, каскад удаления, изоляция организаций и роли.
use e2e::Stack;
use serde_json::{Value, json};

const BOSS: (&str, &str) = ("boss@acme.io", "boss-password-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-pass-1");
const GLOBEX_BOSS: (&str, &str) = ("boss@globex.io", "globex-password-1");

async fn json_of(res: reqwest::Response) -> Value {
    res.json().await.unwrap_or(Value::Null)
}

#[tokio::test]
async fn candidate_to_vacancy_to_stages_across_the_platform() {
    let stack = Stack::start().await;
    stack.seed_tenant("acme", BOSS.0, BOSS.1).await;
    stack
        .seed_tenant("globex", GLOBEX_BOSS.0, GLOBEX_BOSS.1)
        .await;
    let boss = stack.crm_client("acme", BOSS.0, BOSS.1).await;
    stack.add_user(&boss, VIEWER.0, VIEWER.1, "viewer").await;
    let viewer = stack.crm_client("acme", VIEWER.0, VIEWER.1).await;
    let globex = stack
        .crm_client("globex", GLOBEX_BOSS.0, GLOBEX_BOSS.1)
        .await;

    // вакансия и кандидат
    let res = boss
        .post(
            "/api/vacancies",
            json!({"title": "Senior Go", "required_skills": ["Go", "SQL"], "priority": "high"}),
        )
        .await;
    assert_eq!(res.status(), 201);
    let vacancy = json_of(res).await;

    let res = boss
        .post(
            "/api/persons",
            json!({"full_name": "Анна Смирнова", "city": "Казань", "skills": ["Go"]}),
        )
        .await;
    assert_eq!(res.status(), 201);
    let person = json_of(res).await["id"].as_str().unwrap().to_string();

    // отклик создаётся сразу после кандидата - без ожидания и без «не найден»
    let res = boss
        .post(
            "/api/applications",
            json!({"person_id": person, "vacancy_id": vacancy["id"]}),
        )
        .await;
    assert_eq!(res.status(), 201);
    let app = json_of(res).await;
    assert_eq!(app["stage"], "new");
    assert_eq!(app["city"], "Казань");
    assert_eq!(app["match_percent"], 50);
    let app_id = app["id"].as_str().unwrap().to_string();

    // этапы и отказ
    let res = boss
        .post(&format!("/api/applications/{app_id}/next"), json!({}))
        .await;
    assert_eq!(res.status(), 200);
    assert_eq!(json_of(res).await["stage"], "screening");
    let res = boss
        .post(
            &format!("/api/applications/{app_id}/reject"),
            json!({"reason": "Не подходит"}),
        )
        .await;
    assert_eq!(json_of(res).await["stage"], "rejected");
    let detail = json_of(boss.get(&format!("/api/applications/{app_id}")).await).await;
    assert_eq!(detail["history"][0]["kind"], "letter");

    // правка кандидата видна в воронке при следующем чтении
    let res = boss
        .patch(&format!("/api/persons/{person}"), json!({"city": "Москва"}))
        .await;
    assert_eq!(res.status(), 200);
    let page = json_of(boss.get("/api/applications").await).await;
    assert_eq!(page["items"][0]["city"], "Москва");

    // сводка и интервью доступны через BFF
    let overview = json_of(boss.get("/api/overview").await).await;
    assert_eq!(overview["people"], 1);
    assert_eq!(overview["by_stage"][7]["stage"], "rejected");
    assert_eq!(overview["by_stage"][7]["count"], 1);
    let res = boss
        .post(
            "/api/applications",
            json!({"person_id": person, "vacancy_id": json_of(boss.get("/api/vacancies").await).await[0]["id"], "stage": "hr"}),
        )
        .await;
    assert_eq!(res.status(), 409, "the pair is already taken");
    let res = boss
        .post(
            &format!("/api/applications/{app_id}/stage"),
            json!({"stage": "hr"}),
        )
        .await;
    assert_eq!(res.status(), 200);
    let res = boss
        .post(
            &format!("/api/applications/{app_id}/interviews"),
            json!({"at": "2099-01-01T10:00:00+03:00"}),
        )
        .await;
    assert_eq!(res.status(), 200);
    let list = json_of(
        boss.get("/api/interviews?from=2099-01-01T00:00:00Z&to=2099-01-02T00:00:00Z")
            .await,
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["full_name"], "Анна Смирнова");
    let overdue = json_of(boss.get("/api/applications?overdue=true").await).await;
    assert_eq!(overdue["total"], 0);

    // аналитика через BFF: воронка идёт по пройденным этапам, а не по текущему
    let analytics = json_of(boss.get("/api/analytics?days=30").await).await;
    assert_eq!(analytics["period_days"], 30);
    assert_eq!(analytics["funnel"][0]["reached"], 1);
    assert_eq!(
        analytics["funnel"][1]["reached"], 1,
        "screening was reached before the rejection"
    );
    assert_eq!(
        analytics["funnel"][2]["reached"], 1,
        "and so was the HR interview"
    );
    assert_eq!(analytics["funnel"][3]["reached"], 0);
    assert_eq!(analytics["vacancies"][0]["title"], "Senior Go");
    assert_eq!(boss.get("/api/analytics?days=1").await.status(), 400);

    // viewer читает, но не меняет
    assert_eq!(viewer.get("/api/applications").await.status(), 200);
    let res = viewer
        .post(&format!("/api/applications/{app_id}/next"), json!({}))
        .await;
    assert_eq!(res.status(), 403);

    // другая организация ничего не видит
    let page = json_of(globex.get("/api/applications").await).await;
    assert_eq!(page["total"], 0);
    assert_eq!(
        globex
            .get(&format!("/api/applications/{app_id}"))
            .await
            .status(),
        404
    );
    assert_eq!(
        json_of(globex.get("/api/vacancies").await)
            .await
            .as_array()
            .unwrap()
            .len(),
        0
    );

    // удаление кандидата уносит отклик
    assert_eq!(
        boss.delete(&format!("/api/persons/{person}"))
            .await
            .status(),
        204
    );
    let page = json_of(boss.get("/api/applications").await).await;
    assert_eq!(page["total"], 0);
    let vacancies = json_of(boss.get("/api/vacancies").await).await;
    assert_eq!(vacancies[0]["total_applications"], 0);

    stack.stop().await;
}
