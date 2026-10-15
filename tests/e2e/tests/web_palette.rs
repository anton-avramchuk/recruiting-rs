//! Палитра команд и горячие клавиши оболочки в браузере.
mod common;

use std::time::Duration;

use common::*;
use e2e::Stack;
use headless_chrome::browser::tab::ModifierKey;
use serde_json::{Value, json};

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-password-1");

fn text(tab: &Tab, selector: &str) -> String {
    eval_str(
        tab,
        &format!("(document.querySelector({selector:?})||{{}}).innerText||''"),
    )
}

async fn seed() -> (Stack, String) {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    stack.add_user(&owner, VIEWER.0, VIEWER.1, "viewer").await;
    let person: Value = owner
        .post(
            "/api/persons",
            json!({"full_name": "Борис Иванов", "source": "hh.ru", "city": "Казань"}),
        )
        .await
        .json()
        .await
        .unwrap();
    let res = owner
        .post(
            "/api/vacancies",
            json!({"title": "Senior Go", "department": "Платформа", "recruiter": "Мария", "open_days": 3}),
        )
        .await;
    assert_eq!(res.status(), 201);
    let id = person["id"].as_str().unwrap().to_string();
    (stack, id)
}

fn open_palette(tab: &Tab) {
    tab.press_key_with_modifiers("k", Some(&[ModifierKey::Ctrl]))
        .unwrap();
    wait_until(tab, "palette", &exists_js("[data-testid=palette-input]"));
    wait_until(
        tab,
        "input focused",
        "document.activeElement && document.activeElement.dataset.testid === 'palette-input'",
    );
}

fn items(tab: &Tab) -> String {
    text(tab, "[data-testid=palette]")
}

fn settle(tab: &Tab) {
    // экран перерисован, обработчики на месте
    wait_until(tab, "shell", &exists_js("[data-testid=user-label]"));
    std::thread::sleep(Duration::from_millis(400));
}

#[tokio::test]
async fn the_palette_opens_filters_and_runs_a_command_from_the_keyboard() {
    let (stack, _) = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        std::thread::sleep(Duration::from_millis(1500)); // гидратация
        set_marker(&tab);

        // Ctrl K открывает, Esc закрывает
        open_palette(&tab);
        assert!(items(&tab).contains("Аналитика найма"));
        assert!(items(&tab).contains("Горячие клавиши"));
        tab.press_key("Escape").unwrap();
        wait_until(&tab, "closed", &gone_js("[data-testid=palette]"));

        // повторное Ctrl K закрывает
        open_palette(&tab);
        tab.press_key_with_modifiers("k", Some(&[ModifierKey::Ctrl]))
            .unwrap();
        wait_until(&tab, "toggled off", &gone_js("[data-testid=palette]"));

        // кнопка поиска в шапке открывает ту же палитру
        click(&tab, "[data-testid=search-button]");
        wait_until(&tab, "palette by button", &exists_js("[data-testid=palette]"));
        tab.press_key("Escape").unwrap();
        wait_until(&tab, "closed again", &gone_js("[data-testid=palette]"));

        // фильтр и выбор Enter
        open_palette(&tab);
        tab.type_str("аналит").unwrap();
        wait_until(
            &tab,
            "filtered",
            "document.querySelectorAll('[data-testid=palette-item]').length === 1",
        );
        tab.press_key("Enter").unwrap();
        wait_until(&tab, "analytics", "location.pathname === '/analytics'");
        wait_until(&tab, "palette closed", &gone_js("[data-testid=palette]"));
        assert!(marker_alive(&tab), "no reload");

        // стрелки двигают выделение
        open_palette(&tab);
        tab.press_key("ArrowDown").unwrap();
        tab.press_key("ArrowDown").unwrap();
        wait_until(
            &tab,
            "third row selected",
            "document.querySelectorAll('[data-testid=palette-item]')[2].getAttribute('aria-selected') === 'true'",
        );
        tab.press_key("ArrowUp").unwrap();
        wait_until(
            &tab,
            "second row selected",
            "document.querySelectorAll('[data-testid=palette-item]')[1].getAttribute('aria-selected') === 'true'",
        );
        tab.press_key("Escape").unwrap();

        // клик вне окна закрывает
        open_palette(&tab);
        click(&tab, "[data-testid=palette-scrim]");
        wait_until(&tab, "closed by click", &gone_js("[data-testid=palette]"));
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn people_and_vacancies_are_found_and_opened() {
    let (stack, person_id) = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        std::thread::sleep(Duration::from_millis(1500));

        open_palette(&tab);
        tab.type_str("иванов").unwrap();
        wait_until(
            &tab,
            "person found",
            "Array.from(document.querySelectorAll('[data-testid=palette-item]')).some(e => e.innerText.includes('Борис Иванов'))",
        );
        let found = items(&tab);
        assert!(found.contains("ЛЮДИ") || found.contains("Люди"), "{found}");
        assert!(found.contains("Казань"), "{found}");
        tab.press_key("Enter").unwrap();
        wait_until(
            &tab,
            "card",
            &format!("location.pathname === '/candidates/{person_id}'"),
        );

        open_palette(&tab);
        tab.type_str("senior").unwrap();
        wait_until(
            &tab,
            "vacancy found",
            "Array.from(document.querySelectorAll('[data-testid=palette-item]')).some(e => e.innerText.includes('Senior Go'))",
        );
        assert!(items(&tab).contains("Платформа"));
        tab.press_key("Enter").unwrap();
        wait_until(&tab, "vacancies", "location.pathname === '/vacancies'");

        // ничего не нашлось
        open_palette(&tab);
        tab.type_str("яяяя").unwrap();
        wait_until(&tab, "empty", &exists_js("[data-testid=palette-empty]"));
        assert!(text(&tab, "[data-testid=palette-empty]").contains("Ничего не нашлось"));
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn a_failing_people_search_leaves_commands_available() {
    let (stack, _) = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        std::thread::sleep(Duration::from_millis(1500));
        eval(
            &tab,
            "(() => { const f = window.fetch; window.fetch = (u, ...a) => String(u).includes('/persons') \
             ? Promise.reject(new Error('down')) : f(u, ...a); })()",
        );
        open_palette(&tab);
        tab.type_str("аналит").unwrap();
        std::thread::sleep(Duration::from_millis(800));
        assert!(items(&tab).contains("Аналитика найма"));
        assert_eq!(eval(&tab, &count_js("[data-testid=palette-empty]")), 0);
        tab.press_key("Enter").unwrap();
        wait_until(&tab, "analytics", "location.pathname === '/analytics'");
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn shortcuts_move_around_and_stay_out_of_text_fields() {
    let (stack, _) = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        std::thread::sleep(Duration::from_millis(1500));
        set_marker(&tab);

        // G затем буква
        tab.press_key("g").unwrap();
        tab.press_key("c").unwrap();
        wait_until(&tab, "candidates", "location.pathname === '/candidates'");
        settle(&tab);
        tab.press_key("g").unwrap();
        tab.press_key("a").unwrap();
        wait_until(&tab, "analytics", "location.pathname === '/analytics'");
        settle(&tab);
        tab.press_key("g").unwrap();
        tab.press_key("v").unwrap();
        wait_until(&tab, "vacancies", "location.pathname === '/vacancies'");
        settle(&tab);
        tab.press_key("g").unwrap();
        tab.press_key("o").unwrap();
        wait_until(
            &tab,
            "applications",
            "location.pathname === '/applications'",
        );
        settle(&tab);
        tab.press_key("g").unwrap();
        tab.press_key("t").unwrap();
        wait_until(&tab, "today", "location.pathname === '/'");
        assert!(marker_alive(&tab), "no reload");
        settle(&tab);

        // просроченная последовательность не переходит
        tab.press_key("g").unwrap();
        std::thread::sleep(Duration::from_millis(1300));
        tab.press_key("c").unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(path_of(&tab), "/");

        // «/» ведёт в поиск по кандидатам и ставит в него фокус
        tab.press_key("/").unwrap();
        wait_until(&tab, "candidates", "location.pathname === '/candidates'");
        wait_until(
            &tab,
            "search focused",
            "document.activeElement && document.activeElement.dataset.testid === 'search'",
        );

        // в поле ввода клавиши — просто текст
        tab.type_str("gai").unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(path_of(&tab), "/candidates");
        assert_eq!(
            eval_str(&tab, "document.querySelector('[data-testid=search]').value"),
            "gai"
        );

        // I открывает импорт, T — разбор
        tab.press_key("Escape").unwrap();
        eval(&tab, "document.activeElement.blur()");
        tab.press_key("i").unwrap();
        wait_until(&tab, "import", "location.pathname === '/candidates/import'");
        settle(&tab);
        tab.press_key("t").unwrap();
        wait_until(&tab, "triage", "location.pathname === '/triage'");
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn the_cheatsheet_lists_the_working_shortcuts() {
    let (stack, _) = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        std::thread::sleep(Duration::from_millis(1500));

        tab.press_key("?").unwrap();
        wait_until(&tab, "cheatsheet", &exists_js("[data-testid=cheatsheet]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=cheat-row]")), 11);
        let sheet = text(&tab, "[data-testid=cheatsheet]");
        assert!(sheet.contains("Палитра команд"), "{sheet}");
        tab.press_key("Escape").unwrap();
        wait_until(&tab, "closed", &gone_js("[data-testid=cheatsheet]"));

        // из палитры, и закрытие кликом вне окна
        open_palette(&tab);
        tab.type_str("клавиш").unwrap();
        wait_until(
            &tab,
            "item",
            "document.querySelectorAll('[data-testid=palette-item]').length === 1",
        );
        tab.press_key("Enter").unwrap();
        wait_until(&tab, "cheatsheet", &exists_js("[data-testid=cheatsheet]"));
        wait_until(&tab, "palette closed", &gone_js("[data-testid=palette]"));
        eval(
            &tab,
            "document.querySelector('[data-testid=cheat-scrim]').click()",
        );
        wait_until(
            &tab,
            "closed by click",
            &gone_js("[data-testid=cheatsheet]"),
        );
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn a_viewer_has_no_write_commands_and_no_write_shortcuts() {
    let (stack, _) = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", VIEWER.0, VIEWER.1);
        std::thread::sleep(Duration::from_millis(1500));

        open_palette(&tab);
        let list = items(&tab);
        assert!(list.contains("Аналитика найма"), "{list}");
        for banned in [
            "Разобрать отклики",
            "Добавить кандидата",
            "Создать вакансию",
            "Импорт",
        ] {
            assert!(!list.contains(banned), "{banned}: {list}");
        }
        tab.press_key("Escape").unwrap();
        wait_until(&tab, "closed", &gone_js("[data-testid=palette]"));

        tab.press_key("i").unwrap();
        tab.press_key("t").unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(path_of(&tab), "/");

        // переходы доступны
        tab.press_key("g").unwrap();
        tab.press_key("a").unwrap();
        wait_until(&tab, "analytics", "location.pathname === '/analytics'");
    })
    .await
    .unwrap();
    stack.stop().await;
}
