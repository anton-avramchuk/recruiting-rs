mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};

async fn delete(app: &App, token: &str, id: &Value) -> u16 {
    app.send(
        "DELETE",
        &format!("/persons/{}", id.as_str().unwrap()),
        Some(token),
        None,
    )
    .await
    .0
}

async fn fixture() -> (App, TenantId, Value) {
    let app = setup().await;
    let tenant = TenantId::new();
    let recruiter = app.token(tenant, &[Role::Recruiter]);
    let (s, p) = app
        .send(
            "POST",
            "/persons",
            Some(&recruiter),
            Some(json!({"full_name": "Иван Петров"})),
        )
        .await;
    assert_eq!(s, 201);
    for (what, body) in [
        (
            "employments",
            json!([{"company": "A", "position": "Dev", "start": "2020-01-01"}]),
        ),
        ("educations", json!([{"institution": "МГУ"}])),
    ] {
        let (s, _) = app
            .send(
                "PUT",
                &format!("/persons/{}/{what}", p["id"].as_str().unwrap()),
                Some(&recruiter),
                Some(body),
            )
            .await;
        assert_eq!(s, 200);
    }
    (app, tenant, p)
}

async fn rows(app: &App, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&app.db.owner)
        .await
        .unwrap()
}

#[tokio::test]
async fn owner_deletes_the_person_with_the_whole_history() {
    let (app, tenant, p) = fixture().await;
    let owner = app.token(tenant, &[Role::Owner]);

    assert_eq!(delete(&app, &owner, &p["id"]).await, 204);
    let (s, _) = app
        .send(
            "GET",
            &format!("/persons/{}", p["id"].as_str().unwrap()),
            Some(&owner),
            None,
        )
        .await;
    assert_eq!(s, 404);
    for table in ["persons", "employments", "educations"] {
        assert_eq!(rows(&app, table).await, 0, "{table}");
    }
    app.close().await;
}

#[tokio::test]
async fn deleting_twice_or_deleting_unknown_is_404() {
    let (app, tenant, p) = fixture().await;
    let owner = app.token(tenant, &[Role::Owner]);
    assert_eq!(delete(&app, &owner, &p["id"]).await, 204);
    assert_eq!(delete(&app, &owner, &p["id"]).await, 404);
    assert_eq!(
        delete(&app, &owner, &json!(uuid::Uuid::new_v4().to_string())).await,
        404
    );
    app.close().await;
}

#[tokio::test]
async fn recruiters_and_viewers_cannot_delete() {
    let (app, tenant, p) = fixture().await;
    for role in [Role::Recruiter, Role::Viewer] {
        let token = app.token(tenant, &[role]);
        assert_eq!(delete(&app, &token, &p["id"]).await, 403, "{role:?}");
    }
    let (s, _) = app
        .send(
            "DELETE",
            &format!("/persons/{}", p["id"].as_str().unwrap()),
            None,
            None,
        )
        .await;
    assert_eq!(s, 401);
    assert_eq!(rows(&app, "persons").await, 1, "the person is intact");
    app.close().await;
}

#[tokio::test]
async fn another_organization_cannot_delete_the_person() {
    let (app, _tenant, p) = fixture().await;
    let stranger = app.token(TenantId::new(), &[Role::Owner]);
    assert_eq!(delete(&app, &stranger, &p["id"]).await, 404);
    assert_eq!(rows(&app, "persons").await, 1);
    assert_eq!(rows(&app, "employments").await, 1);
    app.close().await;
}
