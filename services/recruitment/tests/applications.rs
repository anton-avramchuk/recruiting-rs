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
    async fn apply(&self, name: &str, skills: &[&str], vacancy: &Value) -> Value {
        let p = self.app.add_person(self.tenant, name, skills).await;
        self.app.add_application(&self.token, p, vacancy).await
    }

    async fn post(&self, id: &Value, action: &str, body: Value) -> (u16, Value) {
        self.app
            .send(
                "POST",
                &format!("/applications/{}/{action}", id.as_str().unwrap()),
                Some(&self.token),
                Some(body),
            )
            .await
    }

    async fn detail(&self, id: &Value) -> Value {
        let (s, v) = self
            .app
            .send(
                "GET",
                &format!("/applications/{}", id.as_str().unwrap()),
                Some(&self.token),
                None,
            )
            .await;
        assert_eq!(s, 200, "{v}");
        v
    }

    async fn stage_of(&self, id: &Value) -> String {
        self.detail(id).await["application"]["stage"]
            .as_str()
            .unwrap()
            .to_string()
    }
}

fn history_texts(detail: &Value) -> Vec<String> {
    detail["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["text"].as_str().unwrap().to_string())
        .collect()
}

// ---------- создание ----------

#[tokio::test]
async fn application_starts_at_new_with_history() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    assert_eq!(a["stage"], "new");
    assert_eq!(a["full_name"], "Анна");
    assert_eq!(a["vacancy_title"], "Go");
    assert_eq!(a["person_applications"], 1);
    let d = c.detail(&a["id"]).await;
    assert_eq!(history_texts(&d), ["Получен отклик"]);
    c.app.close().await;
}

#[tokio::test]
async fn duplicate_application_is_a_conflict() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let p = c.app.add_person(c.tenant, "Анна", &[]).await;
    c.app.add_application(&c.token, p, &v).await;
    let (s, _) = c
        .app
        .send(
            "POST",
            "/applications",
            Some(&c.token),
            Some(json!({"person_id": p, "vacancy_id": v["id"]})),
        )
        .await;
    assert_eq!(s, 409);
    let (_, page) = c
        .app
        .send("GET", "/applications", Some(&c.token), None)
        .await;
    assert_eq!(page["total"], 1);
    c.app.close().await;
}

#[tokio::test]
async fn one_person_can_apply_to_several_vacancies() {
    let c = ctx().await;
    let v1 = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let v2 = c.app.add_vacancy(&c.token, json!({"title": "Rust"})).await;
    let p = c.app.add_person(c.tenant, "Анна", &[]).await;
    c.app.add_application(&c.token, p, &v1).await;
    let second = c.app.add_application(&c.token, p, &v2).await;
    assert_eq!(second["person_applications"], 2);
    let d = c.detail(&second["id"]).await;
    assert_eq!(d["person_applications"].as_array().unwrap().len(), 2);
    c.app.close().await;
}

#[tokio::test]
async fn unknown_person_or_vacancy_is_404() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let p = c.app.add_person(c.tenant, "Анна", &[]).await;
    let (s, _) = c
        .app
        .send(
            "POST",
            "/applications",
            Some(&c.token),
            Some(json!({"person_id": Uuid::new_v4(), "vacancy_id": v["id"]})),
        )
        .await;
    assert_eq!(s, 404);
    let (s, _) = c
        .app
        .send(
            "POST",
            "/applications",
            Some(&c.token),
            Some(json!({"person_id": p, "vacancy_id": Uuid::new_v4()})),
        )
        .await;
    assert_eq!(s, 404);
    c.app.close().await;
}

#[tokio::test]
async fn application_can_start_at_a_given_stage() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let p = c.app.add_person(c.tenant, "Анна", &[]).await;
    let (s, a) = c
        .app
        .send(
            "POST",
            "/applications",
            Some(&c.token),
            Some(json!({"person_id": p, "vacancy_id": v["id"], "stage": "screening"})),
        )
        .await;
    assert_eq!(s, 201, "{a}");
    assert_eq!(a["stage"], "screening");
    let (s, bad) = c
        .app
        .send("POST", "/applications", Some(&c.token), Some(json!({"person_id": c.app.add_person(c.tenant, "Б", &[]).await, "vacancy_id": v["id"], "stage": "done"})))
        .await;
    assert_eq!(s, 400, "{bad}");
    assert_eq!(bad["field"], "stage");
    c.app.close().await;
}

// ---------- этапы ----------

#[tokio::test]
async fn moving_updates_stage_timer_and_history() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    c.app.age_stage(&a["id"], 30).await;
    let (s, moved) = c
        .post(&a["id"], "stage", json!({"stage": "screening"}))
        .await;
    assert_eq!(s, 200, "{moved}");
    assert_eq!(moved["stage"], "screening");
    assert_eq!(moved["hours_on_stage"], 0);
    let d = c.detail(&a["id"]).await;
    assert_eq!(history_texts(&d), ["Этап → Скрининг", "Получен отклик"]);
    c.app.close().await;
}

#[tokio::test]
async fn moving_to_the_same_stage_changes_nothing() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    c.app.age_stage(&a["id"], 30).await;
    let (s, same) = c.post(&a["id"], "stage", json!({"stage": "new"})).await;
    assert_eq!(s, 200);
    assert_eq!(same["hours_on_stage"], 30);
    assert_eq!(history_texts(&c.detail(&a["id"]).await).len(), 1);
    c.app.close().await;
}

#[tokio::test]
async fn rejected_application_can_return() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    c.post(&a["id"], "reject", json!({})).await;
    assert_eq!(c.stage_of(&a["id"]).await, "rejected");
    let (s, _) = c.post(&a["id"], "stage", json!({"stage": "hr"})).await;
    assert_eq!(s, 200);
    assert_eq!(c.stage_of(&a["id"]).await, "hr");
    c.app.close().await;
}

#[tokio::test]
async fn unknown_stage_is_400() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    let (s, p) = c.post(&a["id"], "stage", json!({"stage": "done"})).await;
    assert_eq!(s, 400);
    assert_eq!(p["field"], "stage");
    c.app.close().await;
}

#[tokio::test]
async fn next_moves_one_stage_and_stops_at_the_end() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    c.post(&a["id"], "stage", json!({"stage": "tech"})).await;
    let (s, n) = c.post(&a["id"], "next", json!({})).await;
    assert_eq!(s, 200);
    assert_eq!(n["stage"], "final");
    c.post(&a["id"], "stage", json!({"stage": "offer"})).await;
    let (_, n) = c.post(&a["id"], "next", json!({})).await;
    assert_eq!(n["stage"], "hired");
    let (s, _) = c.post(&a["id"], "next", json!({})).await;
    assert_eq!(s, 409);
    assert_eq!(c.stage_of(&a["id"]).await, "hired");

    let b = c.apply("Борис", &[], &v).await;
    c.post(&b["id"], "reject", json!({})).await;
    let (s, _) = c.post(&b["id"], "next", json!({})).await;
    assert_eq!(s, 409);
    c.app.close().await;
}

#[tokio::test]
async fn rejection_records_reason_and_letter() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    let (s, r) = c
        .post(
            &a["id"],
            "reject",
            json!({"reason": "Закрыли вакансию другим кандидатом"}),
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(r["stage"], "rejected");
    let texts = history_texts(&c.detail(&a["id"]).await);
    assert_eq!(
        texts,
        [
            "Письмо с отказом отправлено",
            "Отказ: Закрыли вакансию другим кандидатом",
            "Получен отклик"
        ]
    );
    c.app.close().await;
}

// ---------- SLA ----------

#[tokio::test]
async fn overdue_follows_the_sla() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let fresh = c.apply("Свежий", &[], &v).await;
    let stale = c.apply("Старый", &[], &v).await;
    c.app.age_stage(&stale["id"], 30).await;
    let tech = c.apply("Тех", &[], &v).await;
    c.post(&tech["id"], "stage", json!({"stage": "tech"})).await;
    c.app.age_stage(&tech["id"], 4 * 24).await;
    let late = c.apply("Долго", &[], &v).await;
    c.post(&late["id"], "stage", json!({"stage": "tech"})).await;
    c.app.age_stage(&late["id"], 6 * 24).await;
    let rejected = c.apply("Отказ", &[], &v).await;
    c.post(&rejected["id"], "reject", json!({})).await;
    c.app.age_stage(&rejected["id"], 30 * 24).await;

    let flag = |d: Value| d["application"]["overdue"].as_bool().unwrap();
    assert!(!flag(c.detail(&fresh["id"]).await));
    assert!(flag(c.detail(&stale["id"]).await));
    assert!(!flag(c.detail(&tech["id"]).await));
    assert!(flag(c.detail(&late["id"]).await));
    assert!(!flag(c.detail(&rejected["id"]).await));
    let stale_detail = c.detail(&stale["id"]).await;
    assert_eq!(stale_detail["application"]["hours_on_stage"], 30);
    c.app.close().await;
}

// ---------- совпадение ----------

#[tokio::test]
async fn match_is_the_share_of_required_skills() {
    let c = ctx().await;
    let v = c
        .app
        .add_vacancy(
            &c.token,
            json!({"title": "Go", "required_skills": ["Go", "K8s", "PostgreSQL"]}),
        )
        .await;
    let a = c.apply("Анна", &["go", "k8S"], &v).await;
    assert_eq!(a["match_percent"], 67);
    let none = c
        .app
        .add_vacancy(&c.token, json!({"title": "Без требований"}))
        .await;
    let b = c.apply("Борис", &["Go"], &none).await;
    assert_eq!(b["match_percent"], json!(null));
    c.app.close().await;
}

// ---------- интервью и передача ----------

#[tokio::test]
async fn interview_is_recorded_with_an_invitation() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    let at = (chrono::Utc::now() + chrono::Duration::days(1))
        .format("%Y-%m-%dT15:00:00+00:00")
        .to_string();
    let (s, r) = c.post(&a["id"], "interviews", json!({"at": at})).await;
    assert_eq!(s, 200, "{r}");
    let d = c.detail(&a["id"]).await;
    let texts = history_texts(&d);
    assert!(texts[0].contains("Приглашение"), "{texts:?}");
    assert!(texts[1].starts_with("Интервью назначено на "), "{texts:?}");
    assert!(texts[1].ends_with("15:00"), "{texts:?}");
    let saved: i64 = sqlx::query_scalar("SELECT count(*) FROM interviews")
        .fetch_one(&c.app.db.owner)
        .await
        .unwrap();
    assert_eq!(saved, 1);
    c.app.close().await;
}

#[tokio::test]
async fn interview_in_the_past_is_400() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    let (s, p) = c
        .post(
            &a["id"],
            "interviews",
            json!({"at": "2020-01-01T10:00:00+00:00"}),
        )
        .await;
    assert_eq!(s, 400);
    assert_eq!(p["field"], "at");
    let (s, p) = c
        .post(&a["id"], "interviews", json!({"at": "завтра"}))
        .await;
    assert_eq!(s, 400);
    assert_eq!(p["field"], "at");
    c.app.close().await;
}

#[tokio::test]
async fn hired_candidate_can_be_handed_off() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    c.post(&a["id"], "stage", json!({"stage": "final"})).await;
    let (s, _) = c
        .post(&a["id"], "handoff", json!({"system": "1С:ЗУП"}))
        .await;
    assert_eq!(s, 409, "not hired yet");
    c.post(&a["id"], "stage", json!({"stage": "hired"})).await;
    let (s, _) = c
        .post(&a["id"], "handoff", json!({"system": "1С:ЗУП"}))
        .await;
    assert_eq!(s, 200);
    assert_eq!(
        history_texts(&c.detail(&a["id"]).await)[0],
        "Карточка передана в 1С:ЗУП"
    );
    let (s, p) = c
        .post(&a["id"], "handoff", json!({"system": "Excel"}))
        .await;
    assert_eq!(s, 400);
    assert_eq!(p["field"], "system");
    c.app.close().await;
}

// ---------- групповые действия ----------

#[tokio::test]
async fn bulk_skips_what_does_not_apply() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    let b = c.apply("Борис", &[], &v).await;
    let done = c.apply("Вера", &[], &v).await;
    c.post(&done["id"], "stage", json!({"stage": "hired"}))
        .await;

    let (s, r) = c
        .app
        .send(
            "POST",
            "/applications/bulk",
            Some(&c.token),
            Some(json!({"action": "next", "ids": [a["id"], b["id"], done["id"], Uuid::new_v4()]})),
        )
        .await;
    assert_eq!(s, 200, "{r}");
    assert_eq!(r["changed"], 2);
    assert_eq!(r["skipped"], 1, "unknown ids are ignored, not counted");
    assert_eq!(c.stage_of(&a["id"]).await, "screening");
    assert_eq!(c.stage_of(&done["id"]).await, "hired");
    c.app.close().await;
}

#[tokio::test]
async fn bulk_reject_and_letter() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    let b = c.apply("Борис", &[], &v).await;
    let (_, r) = c
        .app
        .send(
            "POST",
            "/applications/bulk",
            Some(&c.token),
            Some(json!({"action": "reject", "ids": [a["id"]], "reason": "Не подходит"})),
        )
        .await;
    assert_eq!(r["changed"], 1);
    assert_eq!(c.stage_of(&a["id"]).await, "rejected");
    let (_, r) = c
        .app
        .send(
            "POST",
            "/applications/bulk",
            Some(&c.token),
            Some(json!({"action": "letter", "ids": [b["id"]]})),
        )
        .await;
    assert_eq!(r["changed"], 1);
    assert_eq!(c.stage_of(&b["id"]).await, "new");
    assert_eq!(
        history_texts(&c.detail(&b["id"]).await)[0],
        "Письмо кандидату отправлено"
    );
    c.app.close().await;
}

#[tokio::test]
async fn bulk_limits_and_validation() {
    let c = ctx().await;
    let ids: Vec<Uuid> = (0..201).map(|_| Uuid::new_v4()).collect();
    let (s, _) = c
        .app
        .send(
            "POST",
            "/applications/bulk",
            Some(&c.token),
            Some(json!({"action": "next", "ids": ids})),
        )
        .await;
    assert_eq!(s, 400);
    let (s, p) = c
        .app
        .send(
            "POST",
            "/applications/bulk",
            Some(&c.token),
            Some(json!({"action": "explode", "ids": [Uuid::new_v4()]})),
        )
        .await;
    assert_eq!(s, 400);
    assert_eq!(p["field"], "action");
    c.app.close().await;
}

#[tokio::test]
async fn bulk_does_not_touch_foreign_applications() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    let other = c.app.token(TenantId::new(), &[Role::Owner]);
    let (s, r) = c
        .app
        .send(
            "POST",
            "/applications/bulk",
            Some(&other),
            Some(json!({"action": "next", "ids": [a["id"]]})),
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(r["changed"], 0);
    assert_eq!(c.stage_of(&a["id"]).await, "new");
    c.app.close().await;
}

// ---------- список ----------

#[tokio::test]
async fn list_filters_and_searches() {
    let c = ctx().await;
    let go = c
        .app
        .add_vacancy(&c.token, json!({"title": "Go-разработчик"}))
        .await;
    let qa = c.app.add_vacancy(&c.token, json!({"title": "QA"})).await;
    let p1 = c
        .app
        .add_person_full(
            c.tenant,
            "Анна",
            &["Go"],
            json!({"city": "Москва", "source": "hh.ru"}),
        )
        .await;
    let p2 = c
        .app
        .add_person_full(
            c.tenant,
            "Борис",
            &[],
            json!({"city": "Казань", "source": "Реферал"}),
        )
        .await;
    let a1 = c.app.add_application(&c.token, p1, &go).await;
    c.app.add_application(&c.token, p2, &go).await;
    c.app.add_application(&c.token, p1, &qa).await;
    c.post(&a1["id"], "stage", json!({"stage": "screening"}))
        .await;

    let get = |q: String| {
        let c = &c;
        async move {
            c.app
                .send("GET", &format!("/applications?{q}"), Some(&c.token), None)
                .await
                .1
        }
    };
    assert_eq!(get(String::new()).await["total"], 3);
    assert_eq!(
        get(format!("vacancy_id={}", go["id"].as_str().unwrap())).await["total"],
        2
    );
    assert_eq!(get("stage=screening".into()).await["total"], 1);
    assert_eq!(
        get("source=%D0%A0%D0%B5%D1%84%D0%B5%D1%80%D0%B0%D0%BB".into()).await["total"],
        1
    );
    assert_eq!(get(format!("person_id={p1}")).await["total"], 2);
    // поиск по городу, без учёта регистра, и по названию вакансии
    assert_eq!(
        get("q=%D0%BC%D0%BE%D1%81%D0%BA%D0%B2%D0%B0".into()).await["total"],
        2
    );
    assert_eq!(get("q=qa".into()).await["total"], 1);
    // фильтры комбинируются по «И»
    assert_eq!(
        get("stage=screening&q=%D0%BA%D0%B0%D0%B7%D0%B0%D0%BD%D1%8C".into()).await["total"],
        0
    );
    // спецсимволы поиска буквальные
    assert_eq!(get("q=%25".into()).await["total"], 0);
    c.app.close().await;
}

#[tokio::test]
async fn list_pages_and_sorts() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    for name in ["борис", "Анна", "Вера"] {
        c.apply(name, &[], &v).await;
    }
    let (_, by_name) = c
        .app
        .send("GET", "/applications?sort=name", Some(&c.token), None)
        .await;
    let names: Vec<&str> = by_name["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["full_name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Анна", "борис", "Вера"]);
    let (_, desc) = c
        .app
        .send(
            "GET",
            "/applications?sort=name&dir=desc",
            Some(&c.token),
            None,
        )
        .await;
    assert_eq!(desc["items"][0]["full_name"], "Вера");

    let (_, page) = c
        .app
        .send(
            "GET",
            "/applications?sort=name&page=2&page_size=2",
            Some(&c.token),
            None,
        )
        .await;
    assert_eq!(page["total"], 3);
    assert_eq!(page["page"], 2);
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    c.app.close().await;
}

#[tokio::test]
async fn default_order_puts_longest_waiting_first() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let fresh = c.apply("Свежий", &[], &v).await;
    let old = c.apply("Давний", &[], &v).await;
    c.app.age_stage(&old["id"], 100).await;
    let (_, page) = c
        .app
        .send("GET", "/applications", Some(&c.token), None)
        .await;
    assert_eq!(page["items"][0]["id"], old["id"]);
    assert_eq!(page["items"][1]["id"], fresh["id"]);
    c.app.close().await;
}

#[tokio::test]
async fn sort_by_match_and_salary() {
    let c = ctx().await;
    let v = c
        .app
        .add_vacancy(
            &c.token,
            json!({"title": "Go", "required_skills": ["Go", "SQL"]}),
        )
        .await;
    let weak = c
        .app
        .add_person_full(c.tenant, "Слабый", &[], json!({"expected_salary": 300000}))
        .await;
    let strong = c
        .app
        .add_person_full(
            c.tenant,
            "Сильный",
            &["Go", "SQL"],
            json!({"expected_salary": 100000}),
        )
        .await;
    c.app.add_application(&c.token, weak, &v).await;
    c.app.add_application(&c.token, strong, &v).await;
    let (_, m) = c
        .app
        .send(
            "GET",
            "/applications?sort=match&dir=desc",
            Some(&c.token),
            None,
        )
        .await;
    assert_eq!(m["items"][0]["full_name"], "Сильный");
    let (_, sal) = c
        .app
        .send("GET", "/applications?sort=salary", Some(&c.token), None)
        .await;
    assert_eq!(sal["items"][0]["full_name"], "Сильный");
    c.app.close().await;
}

#[tokio::test]
async fn a_whole_vacancy_board_fits_one_page() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    for n in 0..205 {
        let p = c
            .app
            .add_person(c.tenant, &format!("Человек {n}"), &[])
            .await;
        sqlx::query(
            "INSERT INTO applications (tenant_id, person_id, vacancy_id, stage_changed_at, created_at)
             VALUES ($1, $2, $3, now(), now())",
        )
        .bind(c.tenant.0)
        .bind(p)
        .bind(Uuid::parse_str(v["id"].as_str().unwrap()).unwrap())
        .execute(&c.app.db.owner)
        .await
        .unwrap();
    }
    let (_, page) = c
        .app
        .send(
            "GET",
            &format!(
                "/applications?vacancy_id={}&page_size=200",
                v["id"].as_str().unwrap()
            ),
            Some(&c.token),
            None,
        )
        .await;
    assert_eq!(page["items"].as_array().unwrap().len(), 200);
    assert_eq!(page["total"], 205);
    let (_, clamped) = c
        .app
        .send("GET", "/applications?page_size=999", Some(&c.token), None)
        .await;
    assert_eq!(clamped["page_size"], 200);
    c.app.close().await;
}

#[tokio::test]
async fn bad_list_parameters_are_400() {
    let c = ctx().await;
    for q in [
        "page=0",
        "page_size=0",
        "stage=done",
        "sort=weight",
        "dir=sideways",
    ] {
        let (s, _) = c
            .app
            .send("GET", &format!("/applications?{q}"), Some(&c.token), None)
            .await;
        assert_eq!(s, 400, "{q}");
    }
    c.app.close().await;
}

// ---------- права и изоляция ----------

#[tokio::test]
async fn viewer_reads_but_cannot_change() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    let viewer = c.app.token(c.tenant, &[Role::Viewer]);
    let (s, _) = c
        .app
        .send("GET", "/applications", Some(&viewer), None)
        .await;
    assert_eq!(s, 200);
    let uri = format!("/applications/{}", a["id"].as_str().unwrap());
    let (s, _) = c.app.send("GET", &uri, Some(&viewer), None).await;
    assert_eq!(s, 200);
    for (path, body) in [
        ("stage", json!({"stage": "hr"})),
        ("next", json!({})),
        ("reject", json!({})),
        ("interviews", json!({"at": "2099-01-01T10:00:00+00:00"})),
        ("handoff", json!({"system": "1С:ЗУП"})),
    ] {
        let (s, _) = c
            .app
            .send("POST", &format!("{uri}/{path}"), Some(&viewer), Some(body))
            .await;
        assert_eq!(s, 403, "{path}");
    }
    let (s, _) = c
        .app
        .send(
            "POST",
            "/applications/bulk",
            Some(&viewer),
            Some(json!({"action": "next", "ids": []})),
        )
        .await;
    assert_eq!(s, 403);
    let (s, _) = c
        .app
        .send(
            "POST",
            "/applications",
            Some(&viewer),
            Some(json!({"person_id": Uuid::new_v4(), "vacancy_id": Uuid::new_v4()})),
        )
        .await;
    assert_eq!(s, 403);
    let (s, _) = c.app.send("GET", "/applications", None, None).await;
    assert_eq!(s, 401);
    assert_eq!(c.stage_of(&a["id"]).await, "new");
    c.app.close().await;
}

#[tokio::test]
async fn foreign_application_is_404() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let a = c.apply("Анна", &[], &v).await;
    let other = c.app.token(TenantId::new(), &[Role::Owner]);
    let uri = format!("/applications/{}", a["id"].as_str().unwrap());
    let (s, _) = c.app.send("GET", &uri, Some(&other), None).await;
    assert_eq!(s, 404);
    let (s, _) = c
        .app
        .send(
            "POST",
            &format!("{uri}/stage"),
            Some(&other),
            Some(json!({"stage": "hr"})),
        )
        .await;
    assert_eq!(s, 404);
    let (s, _) = c
        .app
        .send(
            "POST",
            &format!("{uri}/next"),
            Some(&other),
            Some(json!({})),
        )
        .await;
    assert_eq!(s, 404);
    let (_, page) = c.app.send("GET", "/applications", Some(&other), None).await;
    assert_eq!(page["total"], 0);
    assert_eq!(c.stage_of(&a["id"]).await, "new");
    c.app.close().await;
}
