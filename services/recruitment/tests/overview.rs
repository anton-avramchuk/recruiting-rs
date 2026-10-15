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
}

async fn ctx() -> Ctx {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    Ctx { app, tenant, token }
}

impl Ctx {
    async fn apply(&self, name: &str, vacancy: &Value, stage: &str) -> Value {
        let p = self.app.add_person(self.tenant, name, &[]).await;
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
        a
    }

    async fn overview(&self) -> Value {
        let (s, v) = self
            .app
            .send("GET", "/overview", Some(&self.token), None)
            .await;
        assert_eq!(s, 200, "{v}");
        v
    }
}

fn counts(v: &Value) -> Vec<i64> {
    v["by_stage"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["count"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn counters_follow_stages_and_sla() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    c.apply("Н1", &v, "new").await;
    c.apply("Н2", &v, "new").await;
    let waiting = c.apply("Н3", &v, "new").await;
    c.app.age_stage(&waiting["id"], 30).await;
    c.apply("С1", &v, "screening").await;
    c.apply("С2", &v, "screening").await;
    c.apply("О1", &v, "offer").await;
    c.apply("В1", &v, "hired").await;
    c.apply("Отк", &v, "rejected").await;

    let o = c.overview().await;
    assert_eq!(counts(&o), [3, 2, 0, 0, 0, 1, 1, 1]);
    let stages: Vec<&str> = o["by_stage"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["stage"].as_str().unwrap())
        .collect();
    assert_eq!(
        stages,
        [
            "new",
            "screening",
            "hr",
            "tech",
            "final",
            "offer",
            "hired",
            "rejected"
        ]
    );
    assert_eq!(o["new_total"], 3);
    assert_eq!(o["new_waiting_over_day"], 1);
    assert_eq!(o["in_work"], 6);
    assert_eq!(o["overdue"], 1);
    assert_eq!(o["people"], 8);
    c.app.close().await;
}

#[tokio::test]
async fn overdue_in_overview_equals_the_filtered_list_total() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    for (hours, stage) in [
        (30, "new"),
        (4 * 24, "tech"),
        (6 * 24, "tech"),
        (900, "rejected"),
    ] {
        let a = c.apply(&format!("{stage}{hours}"), &v, stage).await;
        c.app.age_stage(&a["id"], hours).await;
    }
    let o = c.overview().await;
    let (_, list) = c
        .app
        .send("GET", "/applications?overdue=true", Some(&c.token), None)
        .await;
    assert_eq!(o["overdue"], list["total"]);
    assert_eq!(o["overdue"], 2);
    c.app.close().await;
}

#[tokio::test]
async fn an_empty_organization_has_zero_counters() {
    let c = ctx().await;
    let o = c.overview().await;
    assert_eq!(counts(&o), [0; 8]);
    for key in [
        "new_total",
        "new_waiting_over_day",
        "in_work",
        "overdue",
        "people",
    ] {
        assert_eq!(o[key], 0, "{key}");
    }
    c.app.close().await;
}

#[tokio::test]
async fn another_organization_is_not_counted() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    c.apply("Анна", &v, "new").await;
    let other = c.app.token(TenantId::new(), &[Role::Owner]);
    let (_, o) = c.app.send("GET", "/overview", Some(&other), None).await;
    assert_eq!(counts(&o), [0; 8]);
    assert_eq!(o["people"], 0);
    c.app.close().await;
}

#[tokio::test]
async fn overview_needs_a_token_and_is_open_to_viewers() {
    let c = ctx().await;
    let viewer = c.app.token(c.tenant, &[Role::Viewer]);
    let (s, _) = c.app.send("GET", "/overview", Some(&viewer), None).await;
    assert_eq!(s, 200);
    let (s, _) = c.app.send("GET", "/overview", None, None).await;
    assert_eq!(s, 401);
    c.app.close().await;
}

// ---------- интервью ----------

/// Интервью кладётся напрямую: через API нельзя назначить время в прошлом.
async fn interview_at(c: &Ctx, application_id: &Value, at: &str) {
    let id: Uuid = application_id.as_str().unwrap().parse().unwrap();
    sqlx::query(
        "INSERT INTO interviews (tenant_id, application_id, scheduled_at, created_at)
         VALUES ($1, $2, $3::timestamptz, now())",
    )
    .bind(c.tenant.0)
    .bind(id)
    .bind(at)
    .execute(&c.app.db.owner)
    .await
    .unwrap();
}

#[tokio::test]
async fn interviews_for_a_period_come_in_time_order() {
    let c = ctx().await;
    let client = c.app.default_client(&c.token).await;
    let contact = c
        .app
        .add_contact(c.tenant, client, "Иванов", Some("HR-директор"))
        .await;
    let v = c
        .app
        .add_vacancy(&c.token, json!({"title": "Go", "contact_id": contact}))
        .await;
    let a = c.apply("Анна", &v, "tech").await;
    let b = c.apply("Борис", &v, "hr").await;
    interview_at(&c, &b["id"], "2026-03-10T15:00:00Z").await;
    interview_at(&c, &a["id"], "2026-03-10T10:00:00Z").await;
    interview_at(&c, &a["id"], "2026-03-11T10:00:00Z").await;

    let (s, list) = c
        .app
        .send(
            "GET",
            "/interviews?from=2026-03-10T00:00:00Z&to=2026-03-11T00:00:00Z",
            Some(&c.token),
            None,
        )
        .await;
    assert_eq!(s, 200, "{list}");
    let items = list.as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["full_name"], "Анна");
    assert_eq!(items[0]["vacancy_title"], "Go");
    assert_eq!(items[0]["contact_name"], "Иванов");
    assert_eq!(items[0]["client_name"], "Заказчик по умолчанию");
    assert_eq!(items[0]["stage"], "tech");
    assert_eq!(items[1]["full_name"], "Борис");
    assert!(
        items[0]["scheduled_at"]
            .as_str()
            .unwrap()
            .starts_with("2026-03-10T10:00")
    );
    c.app.close().await;
}

#[tokio::test]
async fn interview_periods_are_validated() {
    let c = ctx().await;
    for q in [
        "from=2026-03-01T00:00:00Z&to=2026-04-20T00:00:00Z", // длиннее 31 суток
        "from=2026-03-02T00:00:00Z&to=2026-03-01T00:00:00Z", // from позже to
        "from=2026-03-01T00:00:00Z&to=2026-03-01T00:00:00Z", // пустой период
        "from=вчера&to=сегодня",
        "to=2026-03-01T00:00:00Z",
    ] {
        let (s, _) = c
            .app
            .send("GET", &format!("/interviews?{q}"), Some(&c.token), None)
            .await;
        assert_eq!(s, 400, "{q}");
    }
    c.app.close().await;
}

#[tokio::test]
async fn interviews_are_isolated_and_gone_with_the_application() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &v, "hr").await;
    interview_at(&c, &a["id"], "2026-03-10T10:00:00Z").await;
    let q = "/interviews?from=2026-03-10T00:00:00Z&to=2026-03-11T00:00:00Z";
    let other = c.app.token(TenantId::new(), &[Role::Owner]);
    let (_, foreign) = c.app.send("GET", q, Some(&other), None).await;
    assert_eq!(foreign.as_array().unwrap().len(), 0);

    sqlx::query("DELETE FROM people")
        .execute(&c.app.db.owner)
        .await
        .unwrap();
    let (_, after) = c.app.send("GET", q, Some(&c.token), None).await;
    assert_eq!(
        after.as_array().unwrap().len(),
        0,
        "cascade removed the interview"
    );
    let (s, _) = c.app.send("GET", q, None, None).await;
    assert_eq!(s, 401);
    c.app.close().await;
}
