//! Фильтр списка кандидатов по перечню идентификаторов (`ids`).
mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

async fn create(app: &App, token: &str, name: &str, skills: &[&str]) -> Uuid {
    let (s, p) = app
        .send(
            "POST",
            "/persons",
            Some(token),
            Some(json!({"full_name": name, "skills": skills})),
        )
        .await;
    assert_eq!(s, 201, "{p}");
    p["id"].as_str().unwrap().parse().unwrap()
}

fn join(ids: &[Uuid]) -> String {
    ids.iter()
        .map(Uuid::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn names(page: &Value) -> Vec<String> {
    let mut v: Vec<String> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["full_name"].as_str().unwrap().to_string())
        .collect();
    v.sort();
    v
}

#[tokio::test]
async fn only_the_listed_people_are_returned() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let a = create(&app, &token, "Анна", &[]).await;
    let b = create(&app, &token, "Борис", &[]).await;
    create(&app, &token, "Вера", &[]).await;

    let (s, page) = app
        .send(
            "GET",
            &format!("/persons?ids={}", join(&[a, b])),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(names(&page), ["Анна", "Борис"]);
    assert_eq!(page["total"], 2);

    // пустое значение и отсутствие параметра не ограничивают выборку
    for uri in ["/persons?ids=", "/persons"] {
        let (_, all) = app.send("GET", uri, Some(&token), None).await;
        assert_eq!(all["total"], 3, "{uri}");
    }
    app.close().await;
}

#[tokio::test]
async fn ids_combine_with_other_filters_and_pages() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let a = create(&app, &token, "Анна", &["Rust"]).await;
    let b = create(&app, &token, "Борис", &["Go"]).await;
    let c = create(&app, &token, "Вера", &["Go"]).await;
    let ids = join(&[a, b, c]);

    let (_, rust) = app
        .send(
            "GET",
            &format!("/persons?ids={ids}&skill=rust"),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(names(&rust), ["Анна"]);

    let (_, searched) = app
        .send(
            "GET",
            &format!("/persons?ids={ids}&q=вер"),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(names(&searched), ["Вера"]);

    let (_, second) = app
        .send(
            "GET",
            &format!("/persons?ids={ids}&page=2&page_size=2"),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["total"], 3);
    app.close().await;
}

#[tokio::test]
async fn foreign_and_unknown_ids_are_ignored() {
    let app = setup().await;
    let (mine, other) = (TenantId::new(), TenantId::new());
    let token = app.token(mine, &[Role::Recruiter]);
    let foreign_token = app.token(other, &[Role::Recruiter]);
    let own = create(&app, &token, "Своя", &[]).await;
    let foreign = create(&app, &foreign_token, "Чужая", &[]).await;

    let ids = join(&[own, foreign, Uuid::new_v4()]);
    let (s, page) = app
        .send("GET", &format!("/persons?ids={ids}"), Some(&token), None)
        .await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(names(&page), ["Своя"]);
    app.close().await;
}

#[tokio::test]
async fn bad_or_too_many_ids_are_rejected() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let good = Uuid::new_v4();
    let (s, p) = app
        .send(
            "GET",
            &format!("/persons?ids={good},не-идентификатор"),
            Some(&token),
            None,
        )
        .await;
    assert_eq!((s, p["field"].as_str()), (400, Some("ids")), "{p}");

    let many = join(&(0..201).map(|_| Uuid::new_v4()).collect::<Vec<_>>());
    let (s, p) = app
        .send("GET", &format!("/persons?ids={many}"), Some(&token), None)
        .await;
    assert_eq!((s, p["field"].as_str()), (400, Some("ids")), "{p}");

    // ровно предел - можно
    let limit = join(&(0..200).map(|_| Uuid::new_v4()).collect::<Vec<_>>());
    let (s, _) = app
        .send("GET", &format!("/persons?ids={limit}"), Some(&token), None)
        .await;
    assert_eq!(s, 200);
    app.close().await;
}
