//! Синхронизация проекции людей с настоящим сервисом candidate (в процессе теста).
mod common;

use std::sync::Arc;

use candidate::api::AppState as CandidateState;
use common::{App, setup_with_feed};
use platform_auth::{Audience, Claims, KeyPair, Role, TokenSigner, TokenVerifier};
use platform_core::tenant::TenantId;
use platform_db::testing::TestDb;
use recruitment::application::HttpPersonFeed;
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use uuid::Uuid;

const SERVICE_TOKEN: &str = "sync-test-token";

struct Candidate {
    db: TestDb,
    url: String,
    keys: KeyPair,
    server: Option<JoinHandle<()>>,
    http: reqwest::Client,
}

impl Candidate {
    async fn start() -> Self {
        let db = TestDb::create(&candidate::MIGRATOR).await;
        let keys = KeyPair::generate("candidate-test");
        let verifier = Arc::new(TokenVerifier::new(&keys.public_key(), Audience::Tenant));
        let router =
            candidate::api::router(CandidateState::new(db.app.clone(), verifier, SERVICE_TOKEN));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            db,
            url,
            keys,
            server: Some(server),
            http: reqwest::Client::new(),
        }
    }

    fn token(&self, tenant: TenantId) -> String {
        let claims = Claims::new(
            Uuid::new_v4(),
            Some(tenant),
            vec![Role::Owner],
            Audience::Tenant,
            600,
        );
        TokenSigner::new(&self.keys).sign(&claims).unwrap()
    }

    async fn create(&self, tenant: TenantId, body: Value) -> Uuid {
        let res = self
            .http
            .post(format!("{}/persons", self.url))
            .bearer_auth(self.token(tenant))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let v: Value = res.json().await.unwrap();
        v["id"].as_str().unwrap().parse().unwrap()
    }

    async fn patch(&self, tenant: TenantId, id: Uuid, body: Value) {
        let res = self
            .http
            .patch(format!("{}/persons/{id}", self.url))
            .bearer_auth(self.token(tenant))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    }

    async fn delete(&self, tenant: TenantId, id: Uuid) {
        let res = self
            .http
            .delete(format!("{}/persons/{id}", self.url))
            .bearer_auth(self.token(tenant))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 204);
    }

    fn stop(&mut self) {
        if let Some(s) = self.server.take() {
            s.abort();
        }
    }

    async fn close(mut self) {
        self.stop();
        self.db.close().await;
    }
}

struct Env {
    candidate: Candidate,
    app: App,
    tenant: TenantId,
    token: String,
}

async fn env() -> Env {
    let candidate = Candidate::start().await;
    let feed = Arc::new(HttpPersonFeed::new(&candidate.url, SERVICE_TOKEN));
    let app = setup_with_feed(feed).await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    Env {
        candidate,
        app,
        tenant,
        token,
    }
}

impl Env {
    async fn close(self) {
        self.app.close().await;
        self.candidate.close().await;
    }

    async fn applications(&self) -> (axum::http::HeaderMap, Value) {
        let (s, h, v) = self
            .app
            .send_full("GET", "/applications", Some(&self.token), None)
            .await;
        assert_eq!(s, 200, "{v}");
        (h, v)
    }

    async fn projected_people(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM people")
            .fetch_one(&self.app.db.owner)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn a_just_created_candidate_can_apply_at_once() {
    let e = env().await;
    let vacancy = e.app.add_vacancy(&e.token, json!({"title": "Go"})).await;
    let person = e
        .candidate
        .create(
            e.tenant,
            json!({"full_name": "Анна", "city": "Казань", "skills": ["Go"]}),
        )
        .await;
    let (s, a) = e
        .app
        .send(
            "POST",
            "/applications",
            Some(&e.token),
            Some(json!({"person_id": person, "vacancy_id": vacancy["id"]})),
        )
        .await;
    assert_eq!(s, 201, "{a}");
    assert_eq!(a["full_name"], "Анна");
    assert_eq!(a["city"], "Казань");
    e.close().await;
}

#[tokio::test]
async fn candidate_edits_are_visible_on_the_next_read() {
    let e = env().await;
    let vacancy = e
        .app
        .add_vacancy(
            &e.token,
            json!({"title": "Go", "required_skills": ["Go", "SQL"]}),
        )
        .await;
    let person = e
        .candidate
        .create(
            e.tenant,
            json!({"full_name": "Анна", "city": "Казань", "skills": ["Go"]}),
        )
        .await;
    e.app.add_application(&e.token, person, &vacancy).await;
    let (_, before) = e.applications().await;
    assert_eq!(before["items"][0]["city"], "Казань");
    assert_eq!(before["items"][0]["match_percent"], 50);

    e.candidate
        .patch(
            e.tenant,
            person,
            json!({"city": "Москва", "full_name": "Анна Петрова", "skills": ["Go", "SQL"]}),
        )
        .await;
    let (headers, after) = e.applications().await;
    assert_eq!(after["items"][0]["city"], "Москва");
    assert_eq!(after["items"][0]["full_name"], "Анна Петрова");
    assert_eq!(after["items"][0]["match_percent"], 100);
    assert_eq!(headers["x-people-stale"], "false");
    e.close().await;
}

#[tokio::test]
async fn deleting_a_candidate_removes_the_applications() {
    let e = env().await;
    let vacancy = e.app.add_vacancy(&e.token, json!({"title": "Go"})).await;
    let anna = e
        .candidate
        .create(e.tenant, json!({"full_name": "Анна"}))
        .await;
    let boris = e
        .candidate
        .create(e.tenant, json!({"full_name": "Борис"}))
        .await;
    let a = e.app.add_application(&e.token, anna, &vacancy).await;
    e.app.add_application(&e.token, boris, &vacancy).await;
    assert_eq!(e.applications().await.1["total"], 2);

    e.candidate.delete(e.tenant, anna).await;
    let (_, page) = e.applications().await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["full_name"], "Борис");
    let (s, _) = e
        .app
        .send(
            "GET",
            &format!("/applications/{}", a["id"].as_str().unwrap()),
            Some(&e.token),
            None,
        )
        .await;
    assert_eq!(s, 404);
    let (_, vacancies) = e.app.send("GET", "/vacancies", Some(&e.token), None).await;
    assert_eq!(vacancies[0]["total_applications"], 1);
    e.close().await;
}

#[tokio::test]
async fn funnel_keeps_answering_when_candidate_is_down() {
    let mut e = env().await;
    let vacancy = e.app.add_vacancy(&e.token, json!({"title": "Go"})).await;
    let person = e
        .candidate
        .create(e.tenant, json!({"full_name": "Анна", "city": "Казань"}))
        .await;
    e.app.add_application(&e.token, person, &vacancy).await;
    e.applications().await;

    e.candidate.stop();
    tokio::task::yield_now().await;
    let (headers, page) = e.applications().await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["full_name"], "Анна");
    assert_eq!(page["items"][0]["city"], "Казань");
    assert_eq!(headers["x-people-stale"], "true");
    e.close().await;
}

#[tokio::test]
async fn organizations_do_not_see_each_others_people() {
    let e = env().await;
    let other = TenantId::new();
    e.candidate
        .create(other, json!({"full_name": "Чужой"}))
        .await;
    let vacancy = e.app.add_vacancy(&e.token, json!({"title": "Go"})).await;
    let mine = e
        .candidate
        .create(e.tenant, json!({"full_name": "Свой"}))
        .await;
    e.app.add_application(&e.token, mine, &vacancy).await;
    let people: Vec<String> = sqlx::query_scalar("SELECT full_name FROM people ORDER BY full_name")
        .fetch_all(&e.app.db.owner)
        .await
        .unwrap();
    assert_eq!(
        people,
        ["Свой"],
        "only the organization of the caller is synced"
    );
    e.close().await;
}

#[tokio::test]
async fn a_second_reader_skips_sync_while_another_one_is_running() {
    let e = env().await;
    e.candidate
        .create(e.tenant, json!({"full_name": "Анна"}))
        .await;

    // «другой запрос» держит блокировку синхронизации этой организации
    let mut holder = e.app.db.owner.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock(hashtextextended('people_sync:' || $1::text, 0))")
        .bind(e.tenant.0)
        .execute(&mut *holder)
        .await
        .unwrap();
    let (headers, _) = e.applications().await;
    assert_eq!(
        headers["x-people-stale"], "false",
        "skipping is not a failure"
    );
    assert_eq!(
        e.projected_people().await,
        0,
        "nothing was applied meanwhile"
    );

    sqlx::query("SELECT pg_advisory_unlock(hashtextextended('people_sync:' || $1::text, 0))")
        .bind(e.tenant.0)
        .execute(&mut *holder)
        .await
        .unwrap();
    drop(holder);
    e.applications().await;
    assert_eq!(e.projected_people().await, 1);
    e.close().await;
}

#[tokio::test]
async fn long_feeds_are_applied_page_by_page() {
    let e = env().await;
    // больше одной страницы ленты (500) - вставка напрямую в базу candidate
    sqlx::query(
        "INSERT INTO persons (tenant_id, full_name, normalized_name)
         SELECT $1, 'Человек ' || n, 'человек ' || n FROM generate_series(1, 620) n",
    )
    .bind(e.tenant.0)
    .execute(&e.candidate.db.owner)
    .await
    .unwrap();
    e.applications().await;
    assert_eq!(e.projected_people().await, 620);
    e.close().await;
}
