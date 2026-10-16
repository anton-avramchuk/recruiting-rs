//! Люди заказчика: различные люди с откликами на вакансии заказчика.
mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

struct World {
    app: App,
    tenant: TenantId,
    token: String,
    acme: Uuid,
    globex: Uuid,
}

async fn world() -> World {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let acme = app.add_client(tenant, "Acme", false).await;
    let globex = app.add_client(tenant, "Globex", false).await;
    World {
        app,
        tenant,
        token,
        acme,
        globex,
    }
}

impl World {
    async fn vacancy(&self, title: &str, client: Uuid) -> Value {
        self.app
            .add_vacancy(&self.token, json!({"title": title, "client_id": client}))
            .await
    }

    async fn person(&self, name: &str) -> Uuid {
        self.app.add_person(self.tenant, name, &[]).await
    }

    async fn apply(&self, person: Uuid, vacancy: &Value) {
        self.app.add_application(&self.token, person, vacancy).await;
    }

    async fn people(&self, query: &str) -> (u16, Value) {
        self.app
            .send(
                "GET",
                &format!("/applications/people{query}"),
                Some(&self.token),
                None,
            )
            .await
    }
}

fn ids(body: &Value) -> Vec<String> {
    let mut v: Vec<String> = body["person_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i.as_str().unwrap().to_string())
        .collect();
    v.sort();
    v
}

#[tokio::test]
async fn only_people_with_applications_to_the_clients_vacancies_are_listed_once() {
    let w = world().await;
    let (go, rust, qa) = (
        w.vacancy("Go", w.acme).await,
        w.vacancy("Rust", w.acme).await,
        w.vacancy("QA", w.globex).await,
    );
    let (anna, boris, vera) = (
        w.person("Анна").await,
        w.person("Борис").await,
        w.person("Вера").await,
    );
    w.apply(anna, &go).await;
    w.apply(anna, &rust).await; // дважды у одного заказчика
    w.apply(boris, &rust).await;
    w.apply(vera, &qa).await;
    w.apply(anna, &qa).await; // и у другого

    let (s, acme) = w.people(&format!("?client_id={}", w.acme)).await;
    assert_eq!(s, 200, "{acme}");
    let mut expected = vec![anna.to_string(), boris.to_string()];
    expected.sort();
    assert_eq!(ids(&acme), expected);
    assert_eq!(acme["truncated"], false);

    let (_, globex) = w.people(&format!("?client_id={}", w.globex)).await;
    let mut expected = vec![anna.to_string(), vera.to_string()];
    expected.sort();
    assert_eq!(ids(&globex), expected);
    w.app.close().await;
}

#[tokio::test]
async fn a_client_without_applications_gives_an_empty_list() {
    let w = world().await;
    w.vacancy("Go", w.acme).await;
    let (s, body) = w.people(&format!("?client_id={}", w.acme)).await;
    assert_eq!(s, 200);
    assert_eq!(ids(&body), Vec::<String>::new());
    assert_eq!(body["truncated"], false);
    w.app.close().await;
}

#[tokio::test]
async fn more_than_two_hundred_people_are_truncated() {
    let w = world().await;
    let vacancy = w.vacancy("Go", w.acme).await;
    let vacancy_id: Uuid = vacancy["id"].as_str().unwrap().parse().unwrap();
    // 201 человек одним запросом мимо API: проекция и отклики кладутся напрямую
    for i in 0..201 {
        let person = Uuid::new_v4();
        sqlx::query("INSERT INTO people (tenant_id, person_id, full_name) VALUES ($1, $2, $3)")
            .bind(w.tenant.0)
            .bind(person)
            .bind(format!("Человек {i}"))
            .execute(&w.app.db.owner)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO applications (tenant_id, person_id, vacancy_id, stage_changed_at, created_at)
             VALUES ($1, $2, $3, now(), now())",
        )
        .bind(w.tenant.0)
        .bind(person)
        .bind(vacancy_id)
        .execute(&w.app.db.owner)
        .await
        .unwrap();
    }
    let (s, body) = w.people(&format!("?client_id={}", w.acme)).await;
    assert_eq!(s, 200);
    assert_eq!(body["person_ids"].as_array().unwrap().len(), 200);
    assert_eq!(body["truncated"], true);
    w.app.close().await;
}

#[tokio::test]
async fn the_client_is_required_and_must_be_an_id() {
    let w = world().await;
    for query in ["", "?client_id=", "?client_id=мусор", "?client_id=123"] {
        let (s, _) = w.people(query).await;
        assert_eq!(s, 400, "{query}");
    }
    w.app.close().await;
}

#[tokio::test]
async fn another_organization_sees_nothing_and_a_viewer_may_read() {
    let w = world().await;
    let vacancy = w.vacancy("Go", w.acme).await;
    let person = w.person("Анна").await;
    w.apply(person, &vacancy).await;

    let other = w.app.token(TenantId::new(), &[Role::Recruiter]);
    let (s, body) = w
        .app
        .send(
            "GET",
            &format!("/applications/people?client_id={}", w.acme),
            Some(&other),
            None,
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(ids(&body), Vec::<String>::new());

    let viewer = w.app.token(w.tenant, &[Role::Viewer]);
    let (s, body) = w
        .app
        .send(
            "GET",
            &format!("/applications/people?client_id={}", w.acme),
            Some(&viewer),
            None,
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(ids(&body).len(), 1);

    let (s, _) = w
        .app
        .send(
            "GET",
            &format!("/applications/people?client_id={}", w.acme),
            None,
            None,
        )
        .await;
    assert_eq!(s, 401);
    w.app.close().await;
}
