mod common;

use common::setup;
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::json;

#[tokio::test]
async fn minimal_vacancy_gets_defaults() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let body = app
        .with_client(&token, json!({"title": "Senior Go-разработчик"}))
        .await;
    let (s, v) = app
        .send("POST", "/vacancies", Some(&token), Some(body))
        .await;
    assert_eq!(s, 201, "{v}");
    assert_eq!(v["title"], "Senior Go-разработчик");
    assert_eq!(v["openings"], 1);
    assert_eq!(v["priority"], "normal");
    assert_eq!(v["open_days"], 0);
    assert_eq!(v["active_applications"], 0);
    assert_eq!(v["total_applications"], 0);
    app.close().await;
}

#[tokio::test]
async fn invalid_values_name_the_field() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    for (body, field) in [
        (json!({"title": "   "}), "title"),
        (json!({"title": "A", "openings": 0}), "openings"),
        (json!({"title": "A", "priority": "urgent"}), "priority"),
        (json!({"title": "A", "open_days": -3}), "open_days"),
    ] {
        let body = app.with_client(&token, body).await;
        let (s, p) = app
            .send("POST", "/vacancies", Some(&token), Some(body))
            .await;
        assert_eq!(s, 400, "{p}");
        assert_eq!(p["field"], field);
    }
    let (_, list) = app.send("GET", "/vacancies", Some(&token), None).await;
    assert_eq!(list.as_array().unwrap().len(), 0, "nothing was created");
    app.close().await;
}

#[tokio::test]
async fn opened_days_are_reported() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let v = app
        .add_vacancy(&token, json!({"title": "A", "open_days": 52}))
        .await;
    assert_eq!(v["open_days"], 52);
    let (_, card) = app
        .send(
            "GET",
            &format!("/vacancies/{}", v["id"].as_str().unwrap()),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(card["open_days"], 52);
    app.close().await;
}

#[tokio::test]
async fn list_counts_applications_and_puts_priority_first() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let low = app
        .add_vacancy(
            &token,
            json!({"title": "Низкий", "priority": "low", "open_days": 90}),
        )
        .await;
    let busy = app
        .add_vacancy(
            &token,
            json!({"title": "Горит", "priority": "high", "open_days": 5}),
        )
        .await;
    let older = app
        .add_vacancy(&token, json!({"title": "Обычный старый", "open_days": 30}))
        .await;
    let newer = app
        .add_vacancy(&token, json!({"title": "Обычный новый", "open_days": 2}))
        .await;

    let mut ids = vec![];
    for name in ["Анна", "Борис", "Вера"] {
        let p = app.add_person(tenant, name, &[]).await;
        ids.push(app.add_application(&token, p, &busy).await);
    }
    app.send(
        "POST",
        &format!("/applications/{}/reject", ids[1]["id"].as_str().unwrap()),
        Some(&token),
        Some(json!({})),
    )
    .await;
    app.send(
        "POST",
        &format!("/applications/{}/stage", ids[2]["id"].as_str().unwrap()),
        Some(&token),
        Some(json!({"stage": "hired"})),
    )
    .await;

    let (s, list) = app.send("GET", "/vacancies", Some(&token), None).await;
    assert_eq!(s, 200);
    let titles: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        ["Горит", "Обычный старый", "Обычный новый", "Низкий"]
    );
    assert_eq!(list[0]["active_applications"], 1);
    assert_eq!(list[0]["total_applications"], 3);
    assert_eq!(list[1]["total_applications"], 0);
    let _ = (low, older, newer);
    app.close().await;
}

#[tokio::test]
async fn patch_changes_given_fields_only() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let v = app
        .add_vacancy(
            &token,
            json!({"title": "A", "city": "Казань", "recruiter": "Мария", "required_skills": ["Go"]}),
        )
        .await;
    let uri = format!("/vacancies/{}", v["id"].as_str().unwrap());
    let (s, updated) = app
        .send(
            "PATCH",
            &uri,
            Some(&token),
            Some(json!({"openings": 3, "priority": "high", "recruiter": null, "required_skills": [" Go ", "go", "K8s"]})),
        )
        .await;
    assert_eq!(s, 200, "{updated}");
    assert_eq!(updated["openings"], 3);
    assert_eq!(updated["priority"], "high");
    assert_eq!(updated["recruiter"], json!(null));
    assert_eq!(updated["city"], "Казань");
    assert_eq!(updated["title"], "A");
    assert_eq!(updated["required_skills"], json!(["Go", "K8s"]));

    let (s, p) = app
        .send("PATCH", &uri, Some(&token), Some(json!({"openings": 0})))
        .await;
    assert_eq!(s, 400);
    assert_eq!(p["field"], "openings");
    let (_, card) = app.send("GET", &uri, Some(&token), None).await;
    assert_eq!(card["openings"], 3, "failed patch changes nothing");
    app.close().await;
}

#[tokio::test]
async fn another_organization_gets_404() {
    let app = setup().await;
    let a = app.token(TenantId::new(), &[Role::Owner]);
    let b = app.token(TenantId::new(), &[Role::Owner]);
    let v = app.add_vacancy(&a, json!({"title": "A"})).await;
    let uri = format!("/vacancies/{}", v["id"].as_str().unwrap());
    let (s, _) = app.send("GET", &uri, Some(&b), None).await;
    assert_eq!(s, 404);
    let (s, _) = app
        .send("PATCH", &uri, Some(&b), Some(json!({"openings": 5})))
        .await;
    assert_eq!(s, 404);
    let (_, list) = app.send("GET", "/vacancies", Some(&b), None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
    let (_, card) = app.send("GET", &uri, Some(&a), None).await;
    assert_eq!(card["openings"], 1);
    app.close().await;
}

#[tokio::test]
async fn permissions_follow_roles() {
    let app = setup().await;
    let tenant = TenantId::new();
    let viewer = app.token(tenant, &[Role::Viewer]);
    let recruiter = app.token(tenant, &[Role::Recruiter]);
    let v = app.add_vacancy(&recruiter, json!({"title": "A"})).await;

    let (s, _) = app.send("GET", "/vacancies", Some(&viewer), None).await;
    assert_eq!(s, 200);
    let (s, _) = app
        .send(
            "POST",
            "/vacancies",
            Some(&viewer),
            Some(json!({"title": "B"})),
        )
        .await;
    assert_eq!(s, 403);
    let uri = format!("/vacancies/{}", v["id"].as_str().unwrap());
    let (s, _) = app
        .send("PATCH", &uri, Some(&viewer), Some(json!({"openings": 2})))
        .await;
    assert_eq!(s, 403);
    let (s, _) = app.send("GET", "/vacancies", None, None).await;
    assert_eq!(s, 401);
    let (s, _) = app
        .send("POST", "/vacancies", None, Some(json!({"title": "B"})))
        .await;
    assert_eq!(s, 401);
    app.close().await;
}
