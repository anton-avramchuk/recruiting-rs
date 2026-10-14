//! Раздел «Кандидаты» в браузере: список, поиск и фильтры, роли.
mod common;

use std::time::Duration;

use common::*;
use e2e::Stack;
use serde_json::json;

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-password-1");

const ROWS: &str = "[data-testid=persons] tbody tr";

async fn stack_with_people() -> Stack {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    stack.add_user(&owner, VIEWER.0, VIEWER.1, "viewer").await;
    for body in [
        json!({"full_name": "Анна Смирнова", "email": "anna@mail.ru", "skills": ["Rust", "SQL"],
               "source": "hh.ru", "expected_salary": 300000, "declared_experience_years": 6}),
        json!({"full_name": "Борис Иванов", "phone": "+7 903 555 66 77", "skills": ["Go"],
               "source": "Реферал", "declared_experience_years": 2}),
        json!({"full_name": "Виктор Петров", "skills": ["rust", "Docker"], "declared_experience_years": 4}),
    ] {
        let res = owner.post("/api/persons", body).await;
        assert_eq!(res.status(), 201);
    }
    stack
}

#[tokio::test]
async fn list_searches_filters_and_respects_roles() {
    let stack = stack_with_people().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);

        // 1. список: все трое, новые сверху, итог и действия владельца
        tab.navigate_to(&format!("{base}/candidates")).unwrap();
        wait_count(&tab, "three rows", ROWS, 3);
        assert_eq!(
            text_of(&tab, "[data-testid=persons] tbody tr:first-child td:first-child"),
            "Виктор Петров",
            "newest first"
        );
        assert!(eval_str(&tab, "document.querySelector('main').innerText").contains("Найдено: 3"));
        assert!(eval_str(&tab, "document.querySelector('main').innerText").contains("4 года"));
        wait_until(&tab, "add button for the owner", &exists_js("a[href='/candidates/new']"));
        std::thread::sleep(Duration::from_millis(1500)); // гидратация
        set_marker(&tab);

        // 2. поиск обновляет список на месте
        type_into(&tab, "[data-testid=search]", "смирн");
        wait_count(&tab, "search narrows the list", ROWS, 1);
        assert_eq!(
            text_of(&tab, "[data-testid=persons] tbody tr td:first-child"),
            "Анна Смирнова"
        );
        assert!(marker_alive(&tab), "search must not reload the page");

        // 3. ничего не найдено - пустое состояние с подсказкой
        replace_text(&tab, "[data-testid=search]", "zzz-nothing");
        wait_until(
            &tab,
            "nothing found",
            "document.querySelector('main').innerText.includes('Ничего не найдено')",
        );
        assert_eq!(eval(&tab, &count_js(ROWS)), 0);

        // 4. фильтр по навыку (поиск очищен)
        replace_text(&tab, "[data-testid=search]", "");
        wait_count(&tab, "all again", ROWS, 3);
        type_into(&tab, "[data-testid=filter-skill]", "go");
        wait_count(&tab, "skill filter", ROWS, 1);
        assert_eq!(
            text_of(&tab, "[data-testid=persons] tbody tr td:first-child"),
            "Борис Иванов"
        );

        // 5. фильтры комбинируются: навык rust и стаж от 5 лет -> только Анна
        replace_text(&tab, "[data-testid=filter-skill]", "rust");
        wait_count(&tab, "rust", ROWS, 2);
        type_into(&tab, "[data-testid=filter-min-experience]", "5");
        wait_count(&tab, "rust and 5+ years", ROWS, 1);
        assert_eq!(
            text_of(&tab, "[data-testid=persons] tbody tr td:first-child"),
            "Анна Смирнова"
        );
        assert!(marker_alive(&tab));

        // 6. строка ведёт в карточку
        wait_until(
            &tab,
            "row links to the card",
            "document.querySelector('[data-testid=persons] tbody tr td a').getAttribute('href').startsWith('/candidates/')",
        );
    })
    .await
    .unwrap();

    // viewer видит данные, но без кнопки создания
    let base = stack.web_crm_url.clone();
    tokio::task::spawn_blocking(move || {
        let browser = launch(); // отдельный профиль: другая сессия
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", VIEWER.0, VIEWER.1);
        tab.navigate_to(&format!("{base}/candidates")).unwrap();
        wait_count(&tab, "viewer sees the list", ROWS, 3);
        assert!(
            eval(&tab, &gone_js("a[href='/candidates/new']")) == serde_json::Value::Bool(true),
            "viewer has no create button"
        );
    })
    .await
    .unwrap();

    stack.stop().await;
}

#[tokio::test]
async fn an_organization_without_candidates_sees_the_empty_state() {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        tab.navigate_to(&format!("{base}/candidates")).unwrap();
        wait_until(
            &tab,
            "empty state",
            "document.querySelector('main').innerText.includes('Кандидатов пока нет')",
        );
        wait_until(
            &tab,
            "owner can add",
            &exists_js("a[href='/candidates/new']"),
        );
    })
    .await
    .unwrap();

    stack.stop().await;
}

// ---------- создание ----------

const FORM_NAME: &str = "input[name=full_name]";

fn fill(tab: &Tab, name: &str, value: &str) {
    type_into(tab, &format!("input[name={name}]"), value);
}

#[tokio::test]
async fn recruiter_creates_a_candidate_and_confirms_a_duplicate() {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);

        // кнопка в списке ведёт на форму
        tab.navigate_to(&format!("{base}/candidates")).unwrap();
        wait_until(&tab, "add button", &exists_js("a[href='/candidates/new']"));
        click(&tab, "a[href='/candidates/new']");
        wait_until(&tab, "create form", &exists_js(FORM_NAME));
        assert_eq!(path_of(&tab), "/candidates/new");
        set_marker(&tab);

        // 1. ошибка контакта показывается у поля, страница не перезагружается
        fill(&tab, "full_name", "Иван Петров");
        fill(&tab, "email", "not-an-email");
        click(&tab, "[data-testid=submit]");
        wait_until(&tab, "field error", &exists_js("[data-testid=error-email]"));
        assert!(text_of(&tab, "[data-testid=error-email]").contains("email"));
        assert!(marker_alive(&tab), "validation errors must not reload the page");
        assert_eq!(path_of(&tab), "/candidates/new");

        // 2. исправили - кандидат создан, виден в списке
        replace_text(&tab, "input[name=email]", "ivan@mail.ru");
        fill(&tab, "phone", "8 (916) 123-45-67");
        fill(&tab, "skills", "Rust, SQL");
        fill(&tab, "expected_salary", "250000");
        click(&tab, "[data-testid=submit]");
        wait_until(&tab, "redirect to the list", "location.pathname === '/candidates'");
        wait_count(&tab, "new candidate in the list", ROWS, 1);
        assert_eq!(
            text_of(&tab, "[data-testid=persons] tbody tr td:first-child"),
            "Иван Петров"
        );
        assert!(
            eval_str(&tab, "document.querySelector('[data-testid=persons] tbody tr').innerText")
                .contains("+79161234567"),
            "phone is normalized"
        );
        assert!(marker_alive(&tab));

        // 3. тот же email -> предупреждение о дубле со ссылкой на найденного
        click(&tab, "a[href='/candidates/new']");
        wait_until(&tab, "form again", &exists_js(FORM_NAME));
        fill(&tab, "full_name", "Иван П.");
        fill(&tab, "email", "IVAN@mail.ru");
        click(&tab, "[data-testid=submit]");
        wait_until(&tab, "duplicate banner", &exists_js("[data-testid=duplicate-banner]"));
        let banner = text_of(&tab, "[data-testid=duplicate-banner]");
        assert!(banner.contains("Иван Петров"), "{banner}");
        assert!(banner.contains("email"), "{banner}");
        wait_until(
            &tab,
            "link to the existing person",
            "document.querySelector('[data-testid=duplicate-banner] a').getAttribute('href').startsWith('/candidates/')",
        );
        assert_eq!(path_of(&tab), "/candidates/new", "nothing was created yet");

        // отмена оставляет форму и никого не создаёт
        click(&tab, "[data-testid=duplicate-cancel]");
        wait_until(&tab, "banner gone", &gone_js("[data-testid=duplicate-banner]"));

        // 4. подтверждение создаёт второго
        click(&tab, "[data-testid=submit]");
        wait_until(&tab, "banner again", &exists_js("[data-testid=duplicate-banner]"));
        click(&tab, "[data-testid=duplicate-confirm]");
        wait_until(&tab, "list after confirm", "location.pathname === '/candidates'");
        wait_count(&tab, "two candidates", ROWS, 2);
        assert!(marker_alive(&tab));
    })
    .await
    .unwrap();

    stack.stop().await;
}
