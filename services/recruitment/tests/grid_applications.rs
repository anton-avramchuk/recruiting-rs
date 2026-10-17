//! Список откликов на гриде: `GET /applications/grid` - сортировки, фильтры, просрочка, поиск, изоляция.
mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

fn encode(raw: &str) -> String {
    raw.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn filter(value: Value) -> String {
    format!("filter={}", encode(&value.to_string()))
}

struct Ctx {
    app: App,
    tenant: TenantId,
    token: String,
}

async fn ctx() -> Ctx {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    Ctx { app, tenant, token }
}

impl Ctx {
    async fn apply(
        &self,
        name: &str,
        skills: &[&str],
        vacancy: &Value,
        stage: &str,
        hours: i64,
    ) -> Value {
        let p = self.app.add_person(self.tenant, name, skills).await;
        let a = self.app.add_application(&self.token, p, vacancy).await;
        if stage != "new" {
            let (s, _) = self
                .app
                .send(
                    "POST",
                    &format!("/applications/{}/stage", a["id"].as_str().unwrap()),
                    Some(&self.token),
                    Some(json!({"stage": stage})),
                )
                .await;
            assert_eq!(s, 200);
        }
        self.app.age_stage(&a["id"], hours).await;
        a
    }

    async fn get(&self, query: &str) -> (u16, Value) {
        self.app
            .send(
                "GET",
                &format!("/applications/grid?{query}"),
                Some(&self.token),
                None,
            )
            .await
    }
}

fn names(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["full_name"].as_str().unwrap().to_string())
        .collect()
}

/// Две вакансии; пять откликов разного возраста и этапов.
async fn world() -> (Ctx, Value, Value) {
    let c = ctx().await;
    let go = c
        .app
        .add_vacancy(
            &c.token,
            json!({"title": "Go", "required_skills": ["go", "sql"]}),
        )
        .await;
    let qa = c.app.add_vacancy(&c.token, json!({"title": "QA"})).await;
    c.apply("Анна", &["Go", "SQL"], &go, "new", 2).await;
    c.apply("Борис", &["Go"], &go, "new", 30).await;
    c.apply("Вера", &[], &go, "tech", 6 * 24).await;
    c.apply("Глеб", &[], &qa, "hr", 3).await;
    c.apply("Дина", &[], &qa, "rejected", 500).await;
    (c, go, qa)
}

#[tokio::test]
async fn by_default_those_waiting_longest_come_first() {
    let (c, ..) = world().await;
    let (s, page) = c.get("").await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(names(&page), ["Дина", "Вера", "Борис", "Глеб", "Анна"]);
    assert_eq!(page["total"], 5);
    assert_eq!(page["page"], 1);
    assert!(page["items"][0]["stage"].is_string());
    c.app.close().await;
}

#[tokio::test]
async fn sorting_by_name_match_and_stage() {
    let (c, ..) = world().await;
    let (_, by_name) = c.get("sort=full_name:desc").await;
    assert_eq!(names(&by_name)[0], "Дина");
    let (_, by_match) = c.get("sort=match_percent:desc").await;
    assert_eq!(names(&by_match)[0], "Анна");
    let (_, by_stage) = c.get("sort=stage_order:asc&page_size=2").await;
    assert_eq!(by_stage["items"].as_array().unwrap().len(), 2);
    assert_eq!(by_stage["total"], 5);
    c.app.close().await;
}

#[tokio::test]
async fn filters_by_stage_vacancy_overdue_and_text() {
    let (c, go, _) = world().await;
    let tech = filter(json!({"field": "stage", "operator": "eq", "value": "tech"}));
    let (_, page) = c.get(&tech).await;
    assert_eq!(names(&page), ["Вера"]);

    let vacancy = filter(json!({"field": "vacancy_id", "operator": "eq", "value": go["id"]}));
    let (_, page) = c.get(&format!("{vacancy}&sort=full_name:asc")).await;
    assert_eq!(names(&page), ["Анна", "Борис", "Вера"]);

    let overdue = filter(json!({"field": "overdue", "operator": "eq", "value": true}));
    let (_, page) = c.get(&format!("{overdue}&sort=full_name:asc")).await;
    assert_eq!(names(&page), ["Борис", "Вера"]);

    let search = filter(json!({"logic": "or", "filters": [
        {"field": "full_name", "operator": "contains", "value": "глеб"},
        {"field": "vacancy_title", "operator": "contains", "value": "go"}
    ]}));
    let (_, page) = c.get(&format!("{search}&sort=full_name:asc")).await;
    assert_eq!(names(&page), ["Анна", "Борис", "Вера", "Глеб"]);
    c.app.close().await;
}

#[tokio::test]
async fn rows_carry_the_same_fields_as_the_old_list() {
    let (c, ..) = world().await;
    let (_, old) = c
        .app
        .send("GET", "/applications?page_size=50", Some(&c.token), None)
        .await;
    let (_, new) = c.get("page_size=50").await;
    let key = |v: &Value| v["id"].as_str().unwrap().to_string();
    let mut old_items: Vec<&Value> = old["items"].as_array().unwrap().iter().collect();
    let mut new_items: Vec<&Value> = new["items"].as_array().unwrap().iter().collect();
    old_items.sort_by_key(|v| key(v));
    new_items.sort_by_key(|v| key(v));
    assert_eq!(old_items, new_items);
    c.app.close().await;
}

#[tokio::test]
async fn unknown_fields_are_refused_and_the_stale_flag_is_a_header() {
    let (c, ..) = world().await;
    let (s, _) = c.get("sort=password:asc").await;
    assert_eq!(s, 400);
    let bad = filter(json!({"field": "tenant_id", "operator": "eq", "value": "x"}));
    let (s, _) = c.get(&bad).await;
    assert_eq!(s, 400);
    let (_, headers, _) = c
        .app
        .send_full("GET", "/applications/grid", Some(&c.token), None)
        .await;
    assert!(headers.get("x-people-stale").is_some());
    c.app.close().await;
}

#[tokio::test]
async fn another_organization_sees_nothing() {
    let (c, ..) = world().await;
    let other = c.app.token(TenantId(Uuid::new_v4()), &[Role::Owner]);
    let (s, page) = c
        .app
        .send("GET", "/applications/grid", Some(&other), None)
        .await;
    assert_eq!(s, 200);
    assert_eq!(page["total"], 0);
    c.app.close().await;
}
