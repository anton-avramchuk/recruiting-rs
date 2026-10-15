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
    let vacancy = app
        .add_vacancy(&token, json!({"title": "Go", "recruiter": "Мария"}))
        .await;
    Ctx {
        app,
        tenant,
        token,
        vacancy,
    }
}

impl Ctx {
    /// Отклик нового человека; `source` кладётся в проекцию людей.
    async fn apply(&self, name: &str, source: Option<&str>) -> Value {
        let extra = source.map_or(json!({}), |s| json!({"source": s}));
        let person = self
            .app
            .add_person_full(self.tenant, name, &[], extra)
            .await;
        self.app
            .add_application(&self.token, person, &self.vacancy)
            .await
    }

    async fn stage(&self, app: &Value, stage: &str) {
        let (s, _) = self
            .app
            .send(
                "POST",
                &format!("/applications/{}/stage", app["id"].as_str().unwrap()),
                Some(&self.token),
                Some(json!({"stage": stage})),
            )
            .await;
        assert_eq!(s, 200);
    }

    async fn reject(&self, app: &Value) {
        let (s, _) = self
            .app
            .send(
                "POST",
                &format!("/applications/{}/reject", app["id"].as_str().unwrap()),
                Some(&self.token),
                Some(json!({})),
            )
            .await;
        assert_eq!(s, 200);
    }

    async fn analytics(&self, query: &str) -> Value {
        let (s, v) = self
            .app
            .send(
                "GET",
                &format!("/analytics{query}"),
                Some(&self.token),
                None,
            )
            .await;
        assert_eq!(s, 200, "{v}");
        v
    }

    /// Сдвигает создание отклика и его событий в прошлое.
    async fn shift(&self, app: &Value, days: i64) {
        let id: Uuid = app["id"].as_str().unwrap().parse().unwrap();
        sqlx::query(
            "UPDATE applications SET created_at = created_at - make_interval(days => $2::int),
                 stage_changed_at = stage_changed_at - make_interval(days => $2::int) WHERE id = $1",
        )
        .bind(id)
        .bind(days as i32)
        .execute(&self.app.db.owner)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE application_events SET at = at - make_interval(days => $2::int) WHERE application_id = $1",
        )
        .bind(id)
        .bind(days as i32)
        .execute(&self.app.db.owner)
        .await
        .unwrap();
    }
}

fn reached(a: &Value) -> Vec<i64> {
    a["funnel"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["reached"].as_i64().unwrap())
        .collect()
}

fn conversions(a: &Value) -> Vec<i64> {
    a["funnel"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["conversion"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn the_funnel_follows_the_history_not_the_current_stage() {
    let c = ctx().await;
    let a = c.apply("Анна", None).await;
    c.stage(&a, "screening").await;
    c.stage(&a, "hr").await;
    c.reject(&a).await; // дошла до интервью и получила отказ
    c.apply("Борис", None).await; // новый
    let v = c.apply("Вера", None).await;
    c.stage(&v, "hired").await; // перескок на «Выход»

    let r = c.analytics("").await;
    assert_eq!(reached(&r), [3, 2, 2, 1, 1, 1, 1]);
    assert_eq!(conversions(&r), [100, 67, 100, 50, 100, 100, 100]);
    assert_eq!(r["worst_stage"], "tech");
    assert_eq!(r["period_days"], 90);
    let stages: Vec<&str> = r["funnel"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["stage"].as_str().unwrap())
        .collect();
    assert_eq!(
        stages,
        ["new", "screening", "hr", "tech", "final", "offer", "hired"]
    );
    c.app.close().await;
}

#[tokio::test]
async fn an_application_created_at_a_later_stage_is_counted_on_the_way() {
    let c = ctx().await;
    let person = c.app.add_person(c.tenant, "Игорь", &[]).await;
    let (s, _) = c
        .app
        .send(
            "POST",
            "/applications",
            Some(&c.token),
            Some(json!({"person_id": person, "vacancy_id": c.vacancy["id"], "stage": "tech"})),
        )
        .await;
    assert_eq!(s, 201);
    assert_eq!(reached(&c.analytics("").await), [1, 1, 1, 1, 0, 0, 0]);
    c.app.close().await;
}

#[tokio::test]
async fn time_to_hire_and_first_touch_are_medians() {
    let c = ctx().await;
    for (name, days) in [("Первый", 10), ("Второй", 20), ("Третий", 40)] {
        let a = c.apply(name, None).await;
        c.stage(&a, "hired").await;
        // отклик создан `days` суток назад, выход — только что
        let id: Uuid = a["id"].as_str().unwrap().parse().unwrap();
        sqlx::query("UPDATE applications SET created_at = now() - make_interval(days => $2::int) WHERE id = $1")
            .bind(id)
            .bind(days)
            .execute(&c.app.db.owner)
            .await
            .unwrap();
        // первое касание через 2, 4 и 30 часов после создания
        let hours = match days {
            10 => 2,
            20 => 4,
            _ => 30,
        };
        sqlx::query(
            "UPDATE application_events SET at = (SELECT created_at FROM applications WHERE id = $1) + make_interval(hours => $2::int)
             WHERE application_id = $1 AND kind <> 'created'",
        )
        .bind(id)
        .bind(hours)
        .execute(&c.app.db.owner)
        .await
        .unwrap();
    }
    c.apply("Без касаний", None).await;

    let r = c.analytics("?days=365").await;
    let days = r["median_days_to_hire"].as_f64().unwrap();
    assert!((days - 20.0).abs() < 0.01, "{days}");
    let hours = r["median_hours_to_first_touch"].as_f64().unwrap();
    assert!((hours - 4.0).abs() < 0.01, "{hours}");
    c.app.close().await;
}

#[tokio::test]
async fn without_hires_or_touches_the_medians_are_missing() {
    let c = ctx().await;
    c.apply("Анна", None).await;
    let r = c.analytics("").await;
    assert_eq!(r["median_days_to_hire"], Value::Null);
    assert_eq!(r["median_hours_to_first_touch"], Value::Null);
    c.app.close().await;
}

#[tokio::test]
async fn sources_show_the_share_that_reached_the_interview() {
    let c = ctx().await;
    for (name, source, stage) in [
        ("А1", Some("hh.ru"), "hr"),
        ("А2", Some("hh.ru"), "hired"),
        ("А3", Some("hh.ru"), "new"),
        ("А4", Some("hh.ru"), "new"),
        ("Б1", Some("Реферал"), "tech"),
        ("Б2", Some("Реферал"), "screening"),
        ("В1", None, "new"),
    ] {
        let a = c.apply(name, source).await;
        if stage != "new" {
            c.stage(&a, stage).await;
        }
    }
    let r = c.analytics("").await;
    let rows: Vec<(String, i64, i64, i64)> = r["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["source"].as_str().unwrap().to_string(),
                s["total"].as_i64().unwrap(),
                s["interviews"].as_i64().unwrap(),
                s["conversion"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("hh.ru".to_string(), 4, 2, 50),
            ("Реферал".to_string(), 2, 1, 50),
            ("Не указан".to_string(), 1, 0, 0),
        ]
    );
    c.app.close().await;
}

#[tokio::test]
async fn weekly_counts_cover_twelve_weeks_with_zeros() {
    let c = ctx().await;
    c.apply("Эта неделя", None).await;
    let old = c.apply("Три недели назад", None).await;
    c.shift(&old, 21).await;
    let ancient = c.apply("Давно", None).await;
    c.shift(&ancient, 200).await;

    let r = c.analytics("?days=365").await;
    let weekly = r["weekly"].as_array().unwrap();
    assert_eq!(weekly.len(), 12);
    let total: i64 = weekly.iter().map(|w| w["count"].as_i64().unwrap()).sum();
    assert_eq!(total, 2, "older than twelve weeks is not shown");
    assert_eq!(weekly.last().unwrap()["count"], 1);
    assert_eq!(weekly.iter().filter(|w| w["count"] == 0).count(), 10);
    let starts: Vec<&str> = weekly
        .iter()
        .map(|w| w["week_start"].as_str().unwrap())
        .collect();
    let mut sorted = starts.clone();
    sorted.sort();
    assert_eq!(starts, sorted);
    c.app.close().await;
}

#[tokio::test]
async fn the_period_limits_the_applications_but_not_the_repeat_people() {
    let c = ctx().await;
    let recent = c.apply("Свежий", None).await;
    let old = c.apply("Давний", None).await;
    c.shift(&old, 100).await;
    let _ = recent;

    assert_eq!(reached(&c.analytics("?days=90").await)[0], 1);
    assert_eq!(reached(&c.analytics("?days=180").await)[0], 2);
    assert_eq!(c.analytics("").await["period_days"], 90);
    c.app.close().await;
}

#[tokio::test]
async fn people_with_several_applications_are_counted_once() {
    let c = ctx().await;
    let other = c.app.add_vacancy(&c.token, json!({"title": "Rust"})).await;
    let person = c.app.add_person(c.tenant, "Анна", &[]).await;
    c.app.add_application(&c.token, person, &c.vacancy).await;
    c.app.add_application(&c.token, person, &other).await;
    let third = c.app.add_person(c.tenant, "Борис", &[]).await;
    c.app.add_application(&c.token, third, &c.vacancy).await;
    assert_eq!(c.analytics("").await["repeat_people"], 1);
    c.app.close().await;
}

#[tokio::test]
async fn vacancies_show_active_applications_and_the_recruiter() {
    let c = ctx().await;
    let a = c.apply("Анна", None).await;
    let b = c.apply("Борис", None).await;
    let v = c.apply("Вера", None).await;
    c.reject(&b).await;
    c.stage(&v, "hired").await;
    let _ = a;

    let r = c.analytics("").await;
    let load = &r["vacancies"][0];
    assert_eq!(load["title"], "Go");
    assert_eq!(load["recruiter"], "Мария");
    assert_eq!(load["active_applications"], 1);
    assert_eq!(load["open_days"], 0);
    c.app.close().await;
}

#[tokio::test]
async fn an_empty_organization_gets_zeros_and_nulls() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Owner]);
    let (s, r) = app.send("GET", "/analytics", Some(&token), None).await;
    assert_eq!(s, 200);
    assert_eq!(reached(&r), [0; 7]);
    assert_eq!(r["worst_stage"], Value::Null);
    assert_eq!(r["median_days_to_hire"], Value::Null);
    assert_eq!(r["repeat_people"], 0);
    assert_eq!(r["weekly"].as_array().unwrap().len(), 12);
    assert_eq!(r["sources"].as_array().unwrap().len(), 0);
    assert_eq!(r["vacancies"].as_array().unwrap().len(), 0);
    app.close().await;
}

#[tokio::test]
async fn the_period_must_be_between_a_week_and_a_year() {
    let c = ctx().await;
    for q in ["?days=6", "?days=366", "?days=0", "?days=-5", "?days=abc"] {
        let (s, _) = c
            .app
            .send("GET", &format!("/analytics{q}"), Some(&c.token), None)
            .await;
        assert_eq!(s, 400, "{q}");
    }
    for q in ["?days=7", "?days=365"] {
        let (s, _) = c
            .app
            .send("GET", &format!("/analytics{q}"), Some(&c.token), None)
            .await;
        assert_eq!(s, 200, "{q}");
    }
    c.app.close().await;
}

#[tokio::test]
async fn viewers_read_and_strangers_see_nothing() {
    let c = ctx().await;
    c.apply("Анна", Some("hh.ru")).await;
    let viewer = c.app.token(c.tenant, &[Role::Viewer]);
    let (s, r) = c.app.send("GET", "/analytics", Some(&viewer), None).await;
    assert_eq!(s, 200);
    assert_eq!(reached(&r)[0], 1);

    let other = c.app.token(TenantId::new(), &[Role::Owner]);
    let (_, r) = c.app.send("GET", "/analytics", Some(&other), None).await;
    assert_eq!(reached(&r)[0], 0);
    assert_eq!(r["sources"].as_array().unwrap().len(), 0);
    assert_eq!(r["vacancies"].as_array().unwrap().len(), 0);

    let (s, _) = c.app.send("GET", "/analytics", None, None).await;
    assert_eq!(s, 401);
    c.app.close().await;
}
