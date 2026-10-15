//! «Аналитика» в браузере.
mod common;

use std::time::Duration;

use common::*;
use e2e::{CrmClient, Stack};
use serde_json::{Value, json};

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-password-1");

fn text(tab: &Tab, selector: &str) -> String {
    eval_str(
        tab,
        &format!("(document.querySelector({selector:?})||{{}}).innerText||''"),
    )
}

async fn apply(
    owner: &CrmClient,
    vacancy: &Value,
    name: &str,
    source: &str,
    stage: Option<&str>,
) -> Value {
    let person: Value = owner
        .post("/api/persons", json!({"full_name": name, "source": source}))
        .await
        .json()
        .await
        .unwrap();
    let app: Value = owner
        .post(
            "/api/applications",
            json!({"person_id": person["id"], "vacancy_id": vacancy["id"]}),
        )
        .await
        .json()
        .await
        .unwrap();
    if let Some(stage) = stage {
        let res = owner
            .post(
                &format!("/api/applications/{}/stage", app["id"].as_str().unwrap()),
                json!({"stage": stage}),
            )
            .await;
        assert_eq!(res.status(), 200);
    }
    app
}

async fn seed() -> Stack {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    stack.add_user(&owner, VIEWER.0, VIEWER.1, "viewer").await;
    let client = stack.add_client(&owner, "Acme").await;
    let vacancy: Value = owner
        .post(
            "/api/vacancies",
            json!({"title": "Senior Go", "client_id": client, "recruiter": "Мария", "open_days": 60}),
        )
        .await
        .json()
        .await
        .unwrap();
    // Анна дошла до интервью HR и получила отказ, Борис новый, Вера вышла, Дарья — из реферала
    let anna = apply(&owner, &vacancy, "Анна Смирнова", "hh.ru", Some("hr")).await;
    let rejected = owner
        .post(
            &format!("/api/applications/{}/reject", anna["id"].as_str().unwrap()),
            json!({"reason": "мимо"}),
        )
        .await;
    assert_eq!(rejected.status(), 200);
    apply(&owner, &vacancy, "Борис Иванов", "hh.ru", None).await;
    apply(&owner, &vacancy, "Вера Орлова", "hh.ru", Some("hired")).await;
    apply(&owner, &vacancy, "Дарья Белова", "Реферал", None).await;
    stack
}

fn counts(tab: &Tab) -> Vec<String> {
    let v = eval(
        tab,
        "JSON.stringify(Array.from(document.querySelectorAll('[data-testid=funnel-row] [data-testid=funnel-count]')).map(e => e.innerText))",
    );
    serde_json::from_str(v.as_str().unwrap_or("[]")).unwrap_or_default()
}

#[tokio::test]
async fn the_screen_shows_the_funnel_sources_and_load() {
    let stack = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        click(&tab, "nav a[href='/analytics']");
        wait_until(&tab, "funnel", "document.querySelectorAll('[data-testid=funnel-row]').length === 7");
        std::thread::sleep(Duration::from_millis(1500));
        set_marker(&tab);

        // воронка по пройденным этапам: Анна учтена на интервью HR, хотя сейчас в отказе
        assert_eq!(counts(&tab), ["4", "2", "2", "1", "1", "1", "1"]);
        assert_eq!(count_value(&tab, "[data-testid=worst]"), 1);
        assert_eq!(
            eval_str(&tab, "document.querySelector('[data-testid=worst]').closest('[data-testid=funnel-row]').dataset.stage"),
            "screening",
            "ties go to the earlier stage: 4 -> 2 and 2 -> 1 are both 50%"
        );
        assert!(eval(&tab, &count_js("[data-testid=analytics-empty]")) == 0);

        // показатели: срок найма и касание считаются, прочерка нет
        assert!(text(&tab, "[data-testid=stat-hire]").contains("дн"));
        assert!(text(&tab, "[data-testid=stat-touch]").contains(" ч"));
        assert_eq!(text(&tab, "[data-testid=stat-repeat] .v").trim(), "0");

        // источники по конверсии: hh.ru 3 отклика, 2 дошли до интервью
        let sources = text(&tab, "[data-testid=sources] tbody");
        assert!(sources.contains("hh.ru"), "{sources}");
        assert!(sources.contains("67%"), "{sources}");
        assert!(sources.contains("Реферал"), "{sources}");
        assert_eq!(
            eval_str(&tab, "document.querySelector('[data-testid=source-row] td').innerText"),
            "hh.ru",
            "the best conversion goes first"
        );

        // нагрузка на вакансии: давно открытая вакансия выделена
        let load = text(&tab, "[data-testid=load]");
        assert!(load.contains("Senior Go") && load.contains("Мария") && load.contains("60 дн"), "{load}");
        assert_eq!(
            eval(&tab, "document.querySelector('[data-testid=load-row] .urgent') !== null"),
            Value::Bool(true)
        );

        // недели: двенадцать столбцов, все отклики этой недели в последнем
        assert_eq!(eval(&tab, &count_js("[data-testid=week-bar]")), 12);
        assert_eq!(
            eval_str(&tab, "Array.from(document.querySelectorAll('[data-testid=week-bar]')).pop().dataset.count"),
            "4"
        );

        // смена периода без перезагрузки
        click(&tab, "[data-testid=period-30]");
        wait_until(
            &tab,
            "period changed",
            "document.querySelector('[data-testid=period-30]').getAttribute('aria-pressed') === 'true'",
        );
        wait_until(
            &tab,
            "refetched for 30 days",
            "performance.getEntriesByType('resource').some(e => e.name.includes('/api/analytics?days=30'))",
        );
        assert!(marker_alive(&tab), "no reload");
        assert_eq!(counts(&tab).len(), 7);
    })
    .await
    .unwrap();
    stack.stop().await;
}

fn count_value(tab: &Tab, selector: &str) -> i64 {
    eval(tab, &count_js(selector)).as_i64().unwrap_or(-1)
}

#[tokio::test]
async fn an_empty_organization_shows_dashes_and_a_note() {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        tab.navigate_to(&format!("{base}/analytics")).unwrap();
        wait_until(
            &tab,
            "empty note",
            &exists_js("[data-testid=analytics-empty]"),
        );
        assert!(text(&tab, "[data-testid=analytics-empty]").contains("Данных пока нет"));
        assert_eq!(text(&tab, "[data-testid=stat-hire] .v").trim(), "—");
        assert_eq!(text(&tab, "[data-testid=stat-touch] .v").trim(), "—");
        assert_eq!(counts(&tab), ["0"; 7]);
        assert_eq!(count_value(&tab, "[data-testid=worst]"), 0);
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn viewers_can_read_the_analytics() {
    let stack = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", VIEWER.0, VIEWER.1);
        assert_eq!(count_value(&tab, "nav a[href='/analytics']"), 1);
        tab.navigate_to(&format!("{base}/analytics")).unwrap();
        wait_until(
            &tab,
            "funnel",
            "document.querySelectorAll('[data-testid=funnel-row]').length === 7",
        );
        assert_eq!(counts(&tab)[0], "4");
    })
    .await
    .unwrap();
    stack.stop().await;
}
