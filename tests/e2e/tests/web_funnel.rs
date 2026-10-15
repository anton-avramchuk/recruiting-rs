//! Воронка в браузере: доска с перетаскиванием, панель человека, таблица откликов и роли.
mod common;

use std::time::Duration;

use common::*;
use e2e::Stack;
use serde_json::{Value, json};

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-password-1");

const CARDS_NEW: &str = "[data-stage=new] [data-testid=app-card]";
const ROWS: &str = "[data-testid=applications] tbody [data-testid=app-row]";

struct Seed {
    stack: Stack,
    applications: Vec<Value>,
}

async fn seed() -> Seed {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    stack.add_user(&owner, VIEWER.0, VIEWER.1, "viewer").await;

    let vacancy = owner
        .post(
            "/api/vacancies",
            json!({"title": "Senior Go", "required_skills": ["Go", "SQL"], "open_days": 52,
                   "hiring_manager": "Иванов", "priority": "high"}),
        )
        .await;
    assert_eq!(vacancy.status(), 201);
    let vacancy: Value = vacancy.json().await.unwrap();
    let other = owner
        .post("/api/vacancies", json!({"title": "QA инженер"}))
        .await;
    assert_eq!(other.status(), 201);
    let other: Value = other.json().await.unwrap();

    let mut applications = Vec::new();
    for (name, city, skills) in [
        ("Анна Смирнова", "Казань", json!(["Go", "SQL"])),
        ("Борис Иванов", "Москва", json!(["Go"])),
        ("Вера Орлова", "Москва", json!([])),
    ] {
        let person: Value = owner
            .post(
                "/api/persons",
                json!({"full_name": name, "city": city, "skills": skills, "source": "hh.ru"}),
            )
            .await
            .json()
            .await
            .unwrap();
        let app = owner
            .post(
                "/api/applications",
                json!({"person_id": person["id"], "vacancy_id": vacancy["id"]}),
            )
            .await;
        assert_eq!(app.status(), 201);
        applications.push(app.json().await.unwrap());
        if name.starts_with("Анна") {
            // у Анны есть и давний отказ по другой вакансии
            let old = owner
                .post(
                    "/api/applications",
                    json!({"person_id": person["id"], "vacancy_id": other["id"], "stage": "rejected"}),
                )
                .await;
            assert_eq!(old.status(), 201);
        }
    }
    Seed {
        stack,
        applications,
    }
}

fn drag(tab: &Tab, card: &str, column: &str) {
    eval(
        tab,
        &format!(
            "(() => {{ const card = document.querySelector({card:?}); const col = document.querySelector({column:?}); \
             const dt = new DataTransfer(); \
             card.dispatchEvent(new DragEvent('dragstart', {{bubbles: true, dataTransfer: dt}})); \
             col.dispatchEvent(new DragEvent('dragover', {{bubbles: true, cancelable: true, dataTransfer: dt}})); \
             col.dispatchEvent(new DragEvent('drop', {{bubbles: true, cancelable: true, dataTransfer: dt}})); \
             card.dispatchEvent(new DragEvent('dragend', {{bubbles: true}})); }})()"
        ),
    );
}

fn text(tab: &Tab, selector: &str) -> String {
    eval_str(
        tab,
        &format!("(document.querySelector({selector:?})||{{}}).innerText||''"),
    )
}

fn change_select(tab: &Tab, selector: &str, value: &str) {
    wait_until(tab, selector, &exists_js(selector));
    eval(
        tab,
        &format!(
            "(() => {{ const el = document.querySelector({selector:?}); el.value = {value:?}; \
             el.dispatchEvent(new Event('change', {{bubbles: true}})); }})()"
        ),
    );
}

#[tokio::test]
async fn board_panel_and_actions_in_the_browser() {
    let s = seed().await;
    let base = s.stack.web_crm_url.clone();
    let first_app = s.applications[0]["id"].as_str().unwrap().to_string();
    let owner = s.stack.crm_client("acme", OWNER.0, OWNER.1).await;

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);

        // 1. вход ведёт на доску первой вакансии (приоритетная выше); срок открытия подсвечен
        wait_until(&tab, "board", &exists_js("[data-testid=board]"));
        wait_count(&tab, "three new cards", CARDS_NEW, 3);
        assert_eq!(
            count_js_value(&tab, "[data-stage]"),
            8,
            "eight stage columns"
        );
        assert!(text(&tab, "[data-testid=vacancy-facts]").contains("Иванов"));
        assert_eq!(
            eval(
                &tab,
                "document.querySelector('[data-testid=open-days]').classList.contains('urgent')"
            ),
            Value::Bool(true),
            "open longer than 45 days is highlighted"
        );
        std::thread::sleep(Duration::from_millis(1500)); // гидратация
        set_marker(&tab);

        // 2. перетаскивание без перезагрузки страницы
        drag(&tab, CARDS_NEW, "[data-stage=screening]");
        wait_until(
            &tab,
            "moved to screening",
            "document.querySelectorAll('[data-stage=screening] [data-testid=app-card]').length === 1",
        );
        wait_count(&tab, "two left in new", CARDS_NEW, 2);
        assert_eq!(text(&tab, "[data-testid=count-screening]").trim(), "1");
        assert!(marker_alive(&tab), "drag and drop must not reload the page");
        tx.send(()).ok();

        // 3. панель человека: все отклики, профиль, ряд этапов
        click(&tab, "[data-stage=screening] [data-testid=app-card]");
        wait_until(&tab, "panel", &exists_js("[data-testid=panel-name]"));
        wait_until(&tab, "profile", &exists_js("[data-testid=panel-profile]"));
        assert_eq!(text(&tab, "[data-testid=panel-name]"), "Анна Смирнова");
        assert_eq!(count_js_value(&tab, "[data-testid=person-app]"), 2);
        assert!(text(&tab, "[data-testid=panel]").contains("Отказ"));
        assert!(text(&tab, "[data-testid=panel-profile]").contains("Казань"));
        click(&tab, "[data-testid=stage-4]");
        wait_until(
            &tab,
            "stage 4",
            "document.querySelector('[data-testid=panel-stage]').innerText === 'Тех. интервью'",
        );
        assert!(text(&tab, "[data-testid=history]").contains("Этап → Тех. интервью"));
        // доска за панелью обновилась
        wait_until(
            &tab,
            "board follows the panel",
            "document.querySelectorAll('[data-stage=tech] [data-testid=app-card]').length === 1",
        );

        // 4. заметка сохраняется при выходе из поля
        wait_until(&tab, "note field", &exists_js("[data-testid=note]"));
        eval(
            &tab,
            "(() => { const el = document.querySelector('[data-testid=note]'); el.value = 'Перезвонить во вторник'; \
             el.dispatchEvent(new Event('input', {bubbles: true})); el.dispatchEvent(new Event('change', {bubbles: true})); })()",
        );
        wait_until(&tab, "note saved", &exists_js("[data-testid=note-saved]"));

        // 5. интервью: дата и время, запись в истории без перезагрузки
        click(&tab, "[data-testid=act-interview]");
        wait_until(
            &tab,
            "interview input",
            &exists_js("[data-testid=interview-at]"),
        );
        eval(
            &tab,
            "(() => { const el = document.querySelector('[data-testid=interview-at]'); const d = new Date(Date.now() + 2 * 86400000); \
             const p = n => String(n).padStart(2, '0'); \
             el.value = `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T15:00`; \
             el.dispatchEvent(new Event('input', {bubbles: true})); })()",
        );
        click(&tab, "[data-testid=interview-confirm]");
        wait_until(
            &tab,
            "interview in history",
            "document.querySelector('[data-testid=history]').innerText.includes('Интервью назначено на')",
        );
        assert!(text(&tab, "[data-testid=history]").contains("15:00"));
        assert!(marker_alive(&tab));

        // 6. отказ с причиной
        click(&tab, "[data-testid=act-reject]");
        type_into(&tab, "[data-testid=reject-reason]", "Нашли другого");
        click(&tab, "[data-testid=reject-confirm]");
        wait_until(
            &tab,
            "rejected",
            "document.querySelector('[data-testid=panel-stage]').innerText === 'Отказ'",
        );
        assert!(text(&tab, "[data-testid=history]").contains("Отказ: Нашли другого"));

        // 7. Esc закрывает панель
        eval(
            &tab,
            "window.dispatchEvent(new KeyboardEvent('keydown', {key: 'Escape', bubbles: true}))",
        );
        wait_until(&tab, "panel closed", &gone_js("[data-testid=panel]"));
        assert!(marker_alive(&tab));
    });

    rx.await.ok();
    // перенос сохранён на сервере
    let mut moved = false;
    for _ in 0..30 {
        let detail = app_detail(&owner, &first_app).await;
        if detail["application"]["stage"] != "new" {
            moved = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(moved, "drag and drop reached the server");
    handle.await.unwrap();

    let detail = app_detail(&owner, &first_app).await;
    assert_eq!(detail["application"]["stage"], "rejected");
    let person: Value = owner
        .get(&format!(
            "/api/persons/{}",
            detail["application"]["person_id"].as_str().unwrap()
        ))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(person["note"], "Перезвонить во вторник");
    s.stack.stop().await;
}

async fn app_detail(owner: &e2e::CrmClient, id: &str) -> Value {
    owner
        .get(&format!("/api/applications/{id}"))
        .await
        .json::<Value>()
        .await
        .unwrap()
}

fn count_js_value(tab: &Tab, selector: &str) -> i64 {
    eval(tab, &count_js(selector)).as_i64().unwrap_or(-1)
}

#[tokio::test]
async fn applications_table_filters_sorts_and_bulk_actions() {
    let s = seed().await;
    let base = s.stack.web_crm_url.clone();
    let owner_url = base.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &owner_url, "acme", OWNER.0, OWNER.1);
        tab.navigate_to(&format!("{owner_url}/applications")).unwrap();
        wait_count(&tab, "four applications", ROWS, 4);
        assert!(text(&tab, "main").contains("Найдено: 4"));
        std::thread::sleep(Duration::from_millis(1500));
        set_marker(&tab);

        // фильтр по этапу и поиск по городу работают на месте
        change_select(&tab, "[data-testid=filter-stage]", "rejected");
        wait_count(&tab, "rejected only", ROWS, 1);
        change_select(&tab, "[data-testid=filter-stage]", "");
        wait_count(&tab, "all again", ROWS, 4);
        type_into(&tab, "[data-testid=search]", "москва");
        wait_count(&tab, "city search", ROWS, 2);
        assert!(marker_alive(&tab));
        click(&tab, "[data-testid=reset-filters]");
        wait_count(&tab, "filters reset", ROWS, 4);

        // сортировка по имени
        click(&tab, "[data-testid=sort-name]");
        wait_until(
            &tab,
            "sorted by name",
            "document.querySelector('[data-testid=app-row] [data-testid=open-app]').innerText === 'Анна Смирнова'",
        );

        // групповое действие: двое выбраны, «Дальше по воронке»
        change_select(&tab, "[data-testid=filter-stage]", "new");
        wait_count(&tab, "new only", ROWS, 3);
        click(&tab, "[data-testid=select-all]");
        wait_until(&tab, "bulk bar", &exists_js("[data-testid=bulkbar]"));
        assert!(text(&tab, "[data-testid=selected-count]").contains("3"));
        click(&tab, "[data-testid=bulk-next]");
        wait_until(
            &tab,
            "bulk result",
            "!!document.querySelector('[data-testid=bulk-result]') && document.querySelector('[data-testid=bulk-result]').innerText.includes('Изменено: 3')",
        );
        wait_until(&tab, "selection cleared", &gone_js("[data-testid=bulkbar]"));
        wait_until(
            &tab,
            "nobody left in new",
            "document.querySelector('main').innerText.includes('Ничего не найдено')",
        );
        assert!(marker_alive(&tab));
    })
    .await
    .unwrap();
    s.stack.stop().await;
}

#[tokio::test]
async fn viewer_sees_the_funnel_but_cannot_change_it() {
    let s = seed().await;
    let base = s.stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", VIEWER.0, VIEWER.1);
        tab.navigate_to(&format!("{base}/applications")).unwrap();
        wait_count(&tab, "rows", ROWS, 4);
        assert_eq!(eval(&tab, &count_js("[data-testid=select-row]")), 0);
        assert_eq!(eval(&tab, &count_js("[data-testid=select-all]")), 0);
        assert_eq!(eval(&tab, &count_js("[data-testid=new-vacancy]")), 0);

        click(&tab, "[data-testid=app-row] [data-testid=open-app]");
        wait_until(&tab, "panel", &exists_js("[data-testid=panel-name]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=act-next]")), 0);
        assert_eq!(eval(&tab, &count_js("[data-testid=act-reject]")), 0);
    })
    .await
    .unwrap();
    s.stack.stop().await;
}

#[tokio::test]
async fn new_vacancy_and_candidate_with_a_vacancy_from_the_forms() {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        wait_until(
            &tab,
            "empty board",
            "document.querySelector('main').innerText.includes('Вакансий пока нет')",
        );
        std::thread::sleep(Duration::from_millis(1500));

        // создание вакансии открывает её пустую воронку
        click(&tab, "[data-testid=new-vacancy]");
        type_into(&tab, "[data-testid=vf-title]", "Тестировщик");
        click(&tab, "[data-testid=vf-submit]");
        wait_until(&tab, "empty funnel", &exists_js("[data-testid=board]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=app-card]")), 0);
        assert!(
            eval_str(&tab, "document.querySelector('[data-testid=vacancy-select]').selectedOptions[0].textContent")
                .contains("Тестировщик")
        );

        // кандидат сразу на вакансию
        tab.navigate_to(&format!("{base}/candidates/new")).unwrap();
        wait_until(&tab, "form", &exists_js("input[name=full_name]"));
        std::thread::sleep(Duration::from_millis(1500));
        type_into(&tab, "input[name=full_name]", "Игорь Новиков");
        type_into(&tab, "input[name=city]", "Самара");
        wait_until(
            &tab,
            "vacancy option",
            "document.querySelectorAll('[data-testid=vacancy] option').length === 2",
        );
        eval(
            &tab,
            "(() => { const el = document.querySelector('[data-testid=vacancy]'); el.selectedIndex = 1; })()",
        );
        eval(
            &tab,
            "(() => { const el = document.querySelector('[data-testid=stage]'); el.value = 'screening'; })()",
        );
        click(&tab, "[data-testid=submit]");
        wait_until(&tab, "applications page", "location.pathname === '/applications'");
        wait_count(&tab, "the new application", ROWS, 1);
        assert_eq!(text(&tab, "[data-testid=row-stage]"), "Скрининг");
        assert!(text(&tab, "[data-testid=app-row]").contains("Игорь Новиков"));
        assert!(text(&tab, "[data-testid=app-row]").contains("Самара"));
    })
    .await
    .unwrap();
    stack.stop().await;
}
