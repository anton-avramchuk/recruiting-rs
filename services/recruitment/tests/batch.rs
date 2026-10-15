mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

struct Ctx {
    app: App,
    tenant: TenantId,
    token: String,
    vacancy: Value,
}

async fn ctx() -> Ctx {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let vacancy = app.add_vacancy(&token, json!({"title": "Go"})).await;
    Ctx {
        app,
        tenant,
        token,
        vacancy,
    }
}

impl Ctx {
    async fn batch(&self, ids: &[Uuid], extra: Value) -> (u16, Value) {
        let mut body = json!({"vacancy_id": self.vacancy["id"], "person_ids": ids});
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            body[k] = v;
        }
        self.app
            .send("POST", "/applications/batch", Some(&self.token), Some(body))
            .await
    }

    async fn people(&self, n: usize) -> Vec<Uuid> {
        let mut ids = Vec::new();
        for i in 0..n {
            ids.push(
                self.app
                    .add_person(self.tenant, &format!("Человек {i}"), &[])
                    .await,
            );
        }
        ids
    }

    async fn applications(&self) -> Value {
        self.app
            .send(
                "GET",
                "/applications?page_size=200",
                Some(&self.token),
                None,
            )
            .await
            .1
    }
}

#[tokio::test]
async fn applications_are_created_skipped_and_counted() {
    let c = ctx().await;
    let people = c.people(3).await;
    c.app.add_application(&c.token, people[0], &c.vacancy).await;
    let (s, r) = c.batch(&people, json!({})).await;
    assert_eq!(s, 200, "{r}");
    assert_eq!(
        (
            r["created"].as_i64(),
            r["skipped"].as_i64(),
            r["missing"].as_i64()
        ),
        (Some(2), Some(1), Some(0))
    );
    assert_eq!(c.applications().await["total"], 3);
    c.app.close().await;
}

#[tokio::test]
async fn unknown_people_are_counted_as_missing() {
    let c = ctx().await;
    let people = c.people(1).await;
    let (s, r) = c
        .batch(&[people[0], Uuid::new_v4(), Uuid::new_v4()], json!({}))
        .await;
    assert_eq!(s, 200);
    assert_eq!(
        (r["created"].as_i64(), r["missing"].as_i64()),
        (Some(1), Some(2))
    );
    c.app.close().await;
}

#[tokio::test]
async fn repeated_ids_count_once() {
    let c = ctx().await;
    let people = c.people(1).await;
    let (_, r) = c.batch(&[people[0], people[0], people[0]], json!({})).await;
    assert_eq!(
        (r["created"].as_i64(), r["skipped"].as_i64()),
        (Some(1), Some(0))
    );
    c.app.close().await;
}

#[tokio::test]
async fn the_origin_is_written_to_the_history() {
    let c = ctx().await;
    let people = c.people(1).await;
    c.batch(&people, json!({"origin": "kandidaty.xlsx"})).await;
    let page = c.applications().await;
    let id = page["items"][0]["id"].as_str().unwrap();
    let (_, detail) = c
        .app
        .send("GET", &format!("/applications/{id}"), Some(&c.token), None)
        .await;
    assert_eq!(
        detail["history"][0]["text"],
        "Импортирован из файла kandidaty.xlsx"
    );

    let other = c.people(1).await;
    c.batch(&other, json!({})).await;
    let page = c.applications().await;
    let plain = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["person_id"] == json!(other[0]))
        .unwrap();
    let (_, detail) = c
        .app
        .send(
            "GET",
            &format!("/applications/{}", plain["id"].as_str().unwrap()),
            Some(&c.token),
            None,
        )
        .await;
    assert_eq!(detail["history"][0]["text"], "Получен отклик");
    c.app.close().await;
}

#[tokio::test]
async fn the_starting_stage_can_be_chosen() {
    let c = ctx().await;
    let people = c.people(1).await;
    let (s, _) = c.batch(&people, json!({"stage": "screening"})).await;
    assert_eq!(s, 200);
    assert_eq!(c.applications().await["items"][0]["stage"], "screening");
    let more = c.people(1).await;
    let (s, p) = c.batch(&more, json!({"stage": "done"})).await;
    assert_eq!(s, 400);
    assert_eq!(p["field"], "stage");
    c.app.close().await;
}

#[tokio::test]
async fn unknown_vacancy_is_404_and_nothing_is_created() {
    let c = ctx().await;
    let people = c.people(2).await;
    let (s, _) = c
        .app
        .send(
            "POST",
            "/applications/batch",
            Some(&c.token),
            Some(json!({"vacancy_id": Uuid::new_v4(), "person_ids": people})),
        )
        .await;
    assert_eq!(s, 404);
    assert_eq!(c.applications().await["total"], 0);
    c.app.close().await;
}

#[tokio::test]
async fn too_many_ids_are_rejected() {
    let c = ctx().await;
    let ids: Vec<Uuid> = (0..501).map(|_| Uuid::new_v4()).collect();
    let (s, _) = c.batch(&ids, json!({})).await;
    assert_eq!(s, 400);
    c.app.close().await;
}

#[tokio::test]
async fn another_organization_cannot_use_the_vacancy_or_the_people() {
    let c = ctx().await;
    let people = c.people(1).await;
    let other = c.app.token(TenantId::new(), &[Role::Owner]);
    let (s, _) = c
        .app
        .send(
            "POST",
            "/applications/batch",
            Some(&other),
            Some(json!({"vacancy_id": c.vacancy["id"], "person_ids": people})),
        )
        .await;
    assert_eq!(s, 404);
    c.app.close().await;
}

#[tokio::test]
async fn viewers_cannot_create_applications() {
    let c = ctx().await;
    let people = c.people(1).await;
    let viewer = c.app.token(c.tenant, &[Role::Viewer]);
    let (s, _) = c
        .app
        .send(
            "POST",
            "/applications/batch",
            Some(&viewer),
            Some(json!({"vacancy_id": c.vacancy["id"], "person_ids": people})),
        )
        .await;
    assert_eq!(s, 403);
    let (s, _) = c
        .app
        .send("POST", "/applications/batch", None, Some(json!({})))
        .await;
    assert_eq!(s, 401);
    c.app.close().await;
}
