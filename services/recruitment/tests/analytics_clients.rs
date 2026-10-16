//! Аналитика по заказчикам: таблица заказчиков и фильтр `client_id`.
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
    acme_vacancy: Value,
    globex_vacancy: Value,
}

async fn world() -> World {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let acme = app.add_client(tenant, "Acme", false).await;
    let globex = app.add_client(tenant, "Globex", false).await;
    let acme_vacancy = app
        .add_vacancy(
            &token,
            json!({"title": "Go", "client_id": acme, "recruiter": "Мария"}),
        )
        .await;
    let globex_vacancy = app
        .add_vacancy(&token, json!({"title": "Rust", "client_id": globex}))
        .await;
    World {
        app,
        tenant,
        token,
        acme,
        globex,
        acme_vacancy,
        globex_vacancy,
    }
}

impl World {
    async fn apply(&self, name: &str, vacancy: &Value, source: &str, stage: Option<&str>) -> Value {
        let person = self
            .app
            .add_person_full(self.tenant, name, &[], json!({"source": source}))
            .await;
        let application = self.app.add_application(&self.token, person, vacancy).await;
        if let Some(stage) = stage {
            let (s, _) = self
                .app
                .send(
                    "POST",
                    &format!(
                        "/applications/{}/stage",
                        application["id"].as_str().unwrap()
                    ),
                    Some(&self.token),
                    Some(json!({"stage": stage})),
                )
                .await;
            assert_eq!(s, 200);
        }
        application
    }

    async fn report(&self, query: &str) -> (u16, Value) {
        self.app
            .send(
                "GET",
                &format!("/analytics{query}"),
                Some(&self.token),
                None,
            )
            .await
    }
}

fn row<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no row {name} in {}", report["clients"]))
}

#[tokio::test]
async fn the_table_compares_clients() {
    let w = world().await;
    // Acme: 4 отклика, 2 дошли до интервью, один вышел; Globex: 2 отклика без интервью
    w.apply("А1", &w.acme_vacancy, "hh.ru", None).await;
    w.apply("А2", &w.acme_vacancy, "hh.ru", Some("hr")).await;
    w.apply("А3", &w.acme_vacancy, "hh.ru", Some("tech")).await;
    w.apply("А4", &w.acme_vacancy, "hh.ru", Some("hired")).await;
    w.apply("Г1", &w.globex_vacancy, "hh.ru", None).await;
    w.apply("Г2", &w.globex_vacancy, "hh.ru", Some("screening"))
        .await;

    let (s, report) = w.report("").await;
    assert_eq!(s, 200, "{report}");
    let clients = report["clients"].as_array().unwrap();
    assert_eq!(clients.len(), 2);
    assert_eq!(clients[0]["name"], "Acme", "больше откликов - выше");

    let acme = row(&report, "Acme");
    assert_eq!(acme["client_id"], w.acme.to_string());
    assert_eq!(acme["applications"], 4);
    assert_eq!(acme["interviews"], 3);
    assert_eq!(acme["conversion"], 75);
    assert_eq!(acme["hired"], 1);
    assert!(acme["median_days_to_hire"].as_f64().is_some());
    assert_eq!(acme["vacancies"], 1);
    assert_eq!(acme["active_applications"], 3, "вышедший не активен");
    assert_eq!(acme["archived"], false);

    let globex = row(&report, "Globex");
    assert_eq!(
        (
            globex["applications"].as_i64(),
            globex["interviews"].as_i64()
        ),
        (Some(2), Some(0))
    );
    assert_eq!(globex["median_days_to_hire"], Value::Null);
    w.app.close().await;
}

#[tokio::test]
async fn the_filter_narrows_everything_except_the_table() {
    let w = world().await;
    w.apply("А1", &w.acme_vacancy, "hh.ru", Some("hr")).await;
    w.apply("А2", &w.acme_vacancy, "Реферал", None).await;
    w.apply("Г1", &w.globex_vacancy, "hh.ru", Some("tech"))
        .await;

    let (s, all) = w.report("").await;
    assert_eq!(s, 200);
    assert_eq!(all["funnel"][0]["reached"], 3);
    assert_eq!(all["vacancies"].as_array().unwrap().len(), 2);

    let (s, only) = w.report(&format!("?client_id={}", w.acme)).await;
    assert_eq!(s, 200, "{only}");
    assert_eq!(only["funnel"][0]["reached"], 2, "{}", only["funnel"]);
    assert_eq!(only["funnel"][3]["reached"], 0, "тех. интервью у Globex");
    let sources: Vec<_> = only["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["source"].as_str().unwrap())
        .collect();
    assert!(sources.contains(&"hh.ru") && sources.contains(&"Реферал"));
    assert_eq!(only["vacancies"].as_array().unwrap().len(), 1);
    assert_eq!(only["vacancies"][0]["title"], "Go");
    let weeks: i64 = only["weekly"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["count"].as_i64().unwrap())
        .sum();
    assert_eq!(weeks, 2);
    // таблица заказчиков не сужается
    assert_eq!(only["clients"].as_array().unwrap().len(), 2);
    assert_eq!(row(&only, "Globex")["applications"], 1);
    w.app.close().await;
}

#[tokio::test]
async fn repeat_people_are_counted_within_the_selected_client() {
    let w = world().await;
    let person = w
        .app
        .add_person_full(w.tenant, "Повторный", &[], json!({}))
        .await;
    w.app
        .add_application(&w.token, person, &w.acme_vacancy)
        .await;
    w.app
        .add_application(&w.token, person, &w.globex_vacancy)
        .await;

    let (_, all) = w.report("").await;
    assert_eq!(all["repeat_people"], 1);
    let (_, only) = w.report(&format!("?client_id={}", w.acme)).await;
    assert_eq!(
        only["repeat_people"], 0,
        "второй отклик - у другого заказчика"
    );
    w.app.close().await;
}

#[tokio::test]
async fn old_vacancies_without_a_client_get_their_own_row_and_archived_clients_are_marked() {
    let w = world().await;
    let legacy = Uuid::new_v4();
    sqlx::query("INSERT INTO vacancies (id, tenant_id, title, opened_on) VALUES ($1, $2, 'Старая', current_date)")
        .bind(legacy)
        .bind(w.tenant.0)
        .execute(&w.app.db.owner)
        .await
        .unwrap();
    let legacy_vacancy = json!({"id": legacy});
    w.apply("С1", &legacy_vacancy, "hh.ru", None).await;
    sqlx::query("UPDATE clients SET archived = true WHERE client_id = $1")
        .bind(w.globex)
        .execute(&w.app.db.owner)
        .await
        .unwrap();

    let (_, report) = w.report("").await;
    let none = row(&report, "Без заказчика");
    assert_eq!(none["client_id"], Value::Null);
    assert_eq!(
        (none["applications"].as_i64(), none["vacancies"].as_i64()),
        (Some(1), Some(1))
    );
    assert_eq!(row(&report, "Globex")["archived"], true);
    w.app.close().await;
}

#[tokio::test]
async fn a_client_without_vacancies_is_not_listed_and_a_bad_id_is_rejected() {
    let w = world().await;
    w.app.add_client(w.tenant, "Без вакансий", false).await;
    let (_, report) = w.report("").await;
    let names: Vec<_> = report["clients"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(!names.contains(&"Без вакансий"), "{names:?}");

    for bad in ["?client_id=мусор", "?client_id=123"] {
        let (s, _) = w.report(bad).await;
        assert_eq!(s, 400, "{bad}");
    }
    // пустой параметр - как отсутствие фильтра
    let (s, _) = w.report("?client_id=").await;
    assert_eq!(s, 200);
    w.app.close().await;
}

#[tokio::test]
async fn another_organizations_client_gives_empty_metrics_and_no_leak() {
    let w = world().await;
    w.apply("А1", &w.acme_vacancy, "hh.ru", Some("hr")).await;
    let other = TenantId::new();
    let other_token = w.app.token(other, &[Role::Recruiter]);

    let (s, report) = w
        .app
        .send(
            "GET",
            &format!("/analytics?client_id={}", w.acme),
            Some(&other_token),
            None,
        )
        .await;
    assert_eq!(s, 200, "{report}");
    assert_eq!(report["funnel"][0]["reached"], 0);
    assert_eq!(report["vacancies"].as_array().unwrap().len(), 0);
    assert_eq!(report["clients"].as_array().unwrap().len(), 0);
    w.app.close().await;
}

#[tokio::test]
async fn a_viewer_reads_the_client_table() {
    let w = world().await;
    w.apply("А1", &w.acme_vacancy, "hh.ru", None).await;
    let viewer = w.app.token(w.tenant, &[Role::Viewer]);
    let (s, report) = w.app.send("GET", "/analytics", Some(&viewer), None).await;
    assert_eq!(s, 200);
    assert_eq!(row(&report, "Acme")["applications"], 1);
    w.app.close().await;
}
