//! «Сегодня» и режим разбора в браузере.
mod common;

use std::time::Duration;

use common::*;
use e2e::{CrmClient, Stack};
use serde_json::{Value, json};

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-password-1");

struct Seed {
    stack: Stack,
    owner: CrmClient,
    /// Идентификаторы откликов: Анна, Борис, Вера.
    apps: Vec<String>,
    persons: Vec<String>,
}

async fn json_of(res: reqwest::Response) -> Value {
    res.json().await.unwrap()
}

async fn seed() -> Seed {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    stack.add_user(&owner, VIEWER.0, VIEWER.1, "viewer").await;

    let client = stack.add_client(&owner, "Acme").await;
    let go = json_of(
        owner
            .post(
                "/api/vacancies",
                json!({"title": "Senior Go", "client_id": client, "required_skills": ["Go", "SQL"]}),
            )
            .await,
    )
    .await;
    let qa = json_of(
        owner
            .post(
                "/api/vacancies",
                json!({"title": "QA инженер", "client_id": client}),
            )
            .await,
    )
    .await;

    let mut apps = Vec::new();
    let mut persons = Vec::new();
    for (name, skills) in [
        ("Анна Смирнова", json!(["Go", "SQL"])),
        ("Борис Иванов", json!(["Go"])),
        ("Вера Орлова", json!([])),
    ] {
        let person = json_of(
            owner
                .post(
                    "/api/persons",
                    json!({"full_name": name, "skills": skills, "city": "Казань",
                           "summary": "Хорошо знает бэкенд", "source": "hh.ru"}),
                )
                .await,
        )
        .await;
        let app = json_of(
            owner
                .post(
                    "/api/applications",
                    json!({"person_id": person["id"], "vacancy_id": go["id"]}),
                )
                .await,
        )
        .await;
        if name.starts_with("Анна") {
            // давний отказ по другой вакансии
            let old = owner
                .post(
                    "/api/applications",
                    json!({"person_id": person["id"], "vacancy_id": qa["id"], "stage": "rejected"}),
                )
                .await;
            assert_eq!(old.status(), 201);
        }
        apps.push(app["id"].as_str().unwrap().to_string());
        persons.push(person["id"].as_str().unwrap().to_string());
    }
    Seed {
        stack,
        owner,
        apps,
        persons,
    }
}

fn text(tab: &Tab, selector: &str) -> String {
    eval_str(
        tab,
        &format!("(document.querySelector({selector:?})||{{}}).innerText||''"),
    )
}

async fn stage_of(owner: &CrmClient, app: &str) -> String {
    json_of(owner.get(&format!("/api/applications/{app}")).await).await["application"]["stage"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn today_shows_the_summary_queue_and_interviews() {
    let s = seed().await;
    // интервью через минуту-две: попадает в сегодняшние сутки браузера
    let at = (chrono::Utc::now() + chrono::Duration::minutes(2)).to_rfc3339();
    let res = s
        .owner
        .post(
            &format!("/api/applications/{}/interviews", s.apps[1]),
            json!({"at": at}),
        )
        .await;
    assert_eq!(res.status(), 200);
    let base = s.stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);

        // корень — «Сегодня», она первая в навигации
        wait_until(&tab, "today", &exists_js("[data-testid=new-total]"));
        assert_eq!(path_of(&tab), "/");
        assert_eq!(text(&tab, "nav a:first-child"), "Сегодня");
        assert_eq!(text(&tab, "[data-testid=new-total]").trim(), "3");
        assert!(text(&tab, "[data-testid=new-waiting]").contains("0 ждут дольше суток"));
        assert_eq!(text(&tab, "[data-testid=in-work]").trim(), "3");
        assert_eq!(text(&tab, "[data-testid=overdue-total]").trim(), "0");

        // очередь: лучшие по совпадению выше
        wait_count(&tab, "queue rows", "[data-testid=queue-row]", 3);
        assert!(
            text(&tab, "[data-testid=queue-row]").contains("Анна Смирнова"),
            "the best match goes first"
        );

        // интервью сегодня; вакансии в работе
        wait_count(&tab, "interview row", "[data-testid=interview-row]", 1);
        assert!(text(&tab, "[data-testid=interview-row]").contains("Борис Иванов"));
        wait_count(&tab, "vacancy rows", "[data-testid=vacancy-row]", 2);

        // строка очереди открывает панель человека
        std::thread::sleep(Duration::from_millis(1500));
        set_marker(&tab);
        click(&tab, "[data-testid=queue-row]");
        wait_until(&tab, "panel", &exists_js("[data-testid=panel-name]"));
        assert_eq!(text(&tab, "[data-testid=panel-name]"), "Анна Смирнова");
        eval(
            &tab,
            "window.dispatchEvent(new KeyboardEvent('keydown', {key: 'Escape', bubbles: true}))",
        );
        wait_until(&tab, "panel closed", &gone_js("[data-testid=panel]"));

        // «Показать список» ведёт в таблицу с включённым отбором просроченных
        click(&tab, "[data-testid=show-overdue]");
        wait_until(
            &tab,
            "applications",
            "location.pathname === '/applications'",
        );
        assert_eq!(path_of(&tab), "/applications?overdue=1");
        wait_until(
            &tab,
            "filter is on",
            "document.querySelector('[data-testid=filter-overdue]').checked === true",
        );
        wait_until(
            &tab,
            "nothing is overdue yet",
            "document.querySelector('main').innerText.includes('Ничего не найдено')",
        );
        assert!(marker_alive(&tab), "navigation is client-side");
    })
    .await
    .unwrap();
    s.stack.stop().await;
}

#[tokio::test]
async fn triage_walks_the_queue_with_the_keyboard() {
    let s = seed().await;
    let base = s.stack.web_crm_url.clone();

    let blocking = tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        wait_until(&tab, "today", &exists_js("[data-testid=start-triage]"));
        assert!(text(&tab, "[data-testid=start-triage]").contains("мин"));
        std::thread::sleep(Duration::from_millis(1500));

        // клавиша T открывает режим разбора
        tab.press_key("t").unwrap();
        wait_until(&tab, "triage", &exists_js("[data-testid=triage-card]"));
        assert_eq!(path_of(&tab), "/triage");
        wait_until(
            &tab,
            "first card",
            "document.querySelector('[data-testid=triage-name]').innerText === 'Анна Смирнова'",
        );
        assert_eq!(text(&tab, "[data-testid=triage-position]").trim(), "1 / 3");
        // про Анну известно, что по другой вакансии ей уже отказывали
        wait_until(&tab, "seen before", &exists_js("[data-testid=seen-before]"));
        assert!(text(&tab, "[data-testid=seen-before]").contains("QA инженер"));
        wait_until(
            &tab,
            "profile",
            "document.querySelector('[data-testid=triage-card]').innerText.includes('Хорошо знает бэкенд')",
        );
        set_marker(&tab);

        // → в скрининг
        tab.press_key("ArrowRight").unwrap();
        wait_until(
            &tab,
            "second card",
            "document.querySelector('[data-testid=triage-name]').innerText === 'Борис Иванов'",
        );
        assert_eq!(text(&tab, "[data-testid=triage-position]").trim(), "2 / 3");

        // ↓ отложить
        tab.press_key("ArrowDown").unwrap();
        wait_until(
            &tab,
            "third card",
            "document.querySelector('[data-testid=triage-name]').innerText === 'Вера Орлова'",
        );

        // ← отказ с письмом, очередь закончилась
        tab.press_key("ArrowLeft").unwrap();
        wait_until(&tab, "summary", &exists_js("[data-testid=triage-summary]"));
        assert!(text(&tab, "[data-testid=triage-summary]").contains("Разобрано откликов: 2"));
        assert!(marker_alive(&tab), "triage must not reload the page");

        // Esc возвращает на «Сегодня»: отложенный отклик остался новым
        tab.press_key("Escape").unwrap();
        wait_until(&tab, "back to today", "location.pathname === '/'");
        wait_until(
            &tab,
            "one new left",
            "document.querySelector('[data-testid=new-total]') && document.querySelector('[data-testid=new-total]').innerText.trim() === '1'",
        );
    });
    blocking.await.unwrap();

    assert_eq!(stage_of(&s.owner, &s.apps[0]).await, "screening");
    assert_eq!(stage_of(&s.owner, &s.apps[1]).await, "new");
    assert_eq!(stage_of(&s.owner, &s.apps[2]).await, "rejected");
    let detail = json_of(
        s.owner
            .get(&format!("/api/applications/{}", s.apps[2]))
            .await,
    )
    .await;
    assert!(
        detail["history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["text"] == "Письмо с отказом отправлено")
    );
    s.stack.stop().await;
}

#[tokio::test]
async fn a_failed_action_keeps_the_card_on_screen() {
    let s = seed().await;
    let base = s.stack.web_crm_url.clone();
    let (loaded_tx, loaded_rx) = tokio::sync::oneshot::channel::<()>();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();

    let blocking = tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        tab.navigate_to(&format!("{base}/triage")).unwrap();
        wait_until(
            &tab,
            "first card",
            "!!document.querySelector('[data-testid=triage-name]')",
        );
        std::thread::sleep(Duration::from_millis(1500));
        // Анну пропускаем, на Борисе ждём, пока его удалят
        tab.press_key("ArrowRight").unwrap();
        wait_until(
            &tab,
            "second card",
            "document.querySelector('[data-testid=triage-name]').innerText === 'Борис Иванов'",
        );
        loaded_tx.send(()).unwrap();
        go_rx.recv().unwrap();

        tab.press_key("ArrowRight").unwrap();
        wait_until(&tab, "error", &exists_js("[data-testid=triage-error]"));
        assert_eq!(
            text(&tab, "[data-testid=triage-name]"),
            "Борис Иванов",
            "the card stays"
        );
        assert_eq!(text(&tab, "[data-testid=triage-position]").trim(), "2 / 3");
        assert!(
            text(&tab, "[data-testid=triage-pace]").trim() != "—",
            "one response was processed"
        );
    });

    loaded_rx.await.unwrap();
    // Бориса удаляют, воронка узнаёт об этом при следующем чтении
    assert_eq!(
        s.owner
            .delete(&format!("/api/persons/{}", s.persons[1]))
            .await
            .status(),
        204
    );
    assert_eq!(s.owner.get("/api/applications").await.status(), 200);
    go_tx.send(()).unwrap();
    blocking.await.unwrap();
    s.stack.stop().await;
}

#[tokio::test]
async fn viewer_cannot_triage() {
    let s = seed().await;
    let base = s.stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", VIEWER.0, VIEWER.1);
        wait_until(&tab, "today", &exists_js("[data-testid=new-total]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=start-triage]")), 0);
        wait_count(&tab, "queue is visible", "[data-testid=queue-row]", 3);

        tab.navigate_to(&format!("{base}/triage")).unwrap();
        wait_until(
            &tab,
            "forbidden",
            &exists_js("[data-testid=triage-forbidden]"),
        );
        assert_eq!(
            eval(
                &tab,
                &count_js("[data-testid=triage-screen]:not([disabled])")
            ),
            0
        );
    })
    .await
    .unwrap();
    s.stack.stop().await;
}

#[tokio::test]
async fn an_empty_queue_says_so() {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        wait_until(&tab, "today", &exists_js("[data-testid=new-total]"));
        assert!(text(&tab, "[data-testid=queue]").contains("Очередь пуста"));
        wait_until(
            &tab,
            "no interviews",
            "document.querySelector('[data-testid=interviews]').innerText.includes('Интервью не назначены')",
        );
        tab.navigate_to(&format!("{base}/triage")).unwrap();
        wait_until(&tab, "empty triage", &exists_js("[data-testid=triage-empty]"));
    })
    .await
    .unwrap();
    stack.stop().await;
}
