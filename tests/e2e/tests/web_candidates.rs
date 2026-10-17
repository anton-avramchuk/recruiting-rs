//! Раздел «Кандидаты» в браузере: список, поиск и фильтры, роли.
mod common;

use std::time::Duration;

use common::*;
use e2e::Stack;
use serde_json::json;

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-password-1");

const ROWS: &str = "tbody tr[data-testid=grid-row]";

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
            text_of(&tab, "tr[data-testid=grid-row]:first-child td:first-child"),
            "Виктор Петров",
            "newest first"
        );
        assert!(eval_str(&tab, "document.querySelector('main').innerText").contains("1–3 из 3"));
        assert!(eval_str(&tab, "document.querySelector('main').innerText").contains("4 года"));
        wait_until(&tab, "add button for the owner", &exists_js("a[href='/candidates/new']"));
        std::thread::sleep(Duration::from_millis(1500)); // гидратация
        set_marker(&tab);

        // 2. поиск обновляет список на месте
        type_into(&tab, "[data-testid=grid-search]", "смирн");
        wait_count(&tab, "search narrows the list", ROWS, 1);
        assert_eq!(
            text_of(&tab, "tr[data-testid=grid-row] td:first-child"),
            "Анна Смирнова"
        );
        assert!(marker_alive(&tab), "search must not reload the page");

        // 3. ничего не найдено - пустое состояние с подсказкой
        replace_text(&tab, "[data-testid=grid-search]", "zzz-nothing");
        wait_until(
            &tab,
            "nothing found",
            "document.querySelector('main').innerText.includes('Ничего не найдено')",
        );
        assert_eq!(eval(&tab, &count_js(ROWS)), 0);

        // 4. фильтр по навыку (поиск очищен)
        replace_text(&tab, "[data-testid=grid-search]", "");
        wait_count(&tab, "all again", ROWS, 3);
        type_into(&tab, "[data-testid=grid-filter-skills]", "go");
        wait_count(&tab, "skill filter", ROWS, 1);
        assert_eq!(
            text_of(&tab, "tr[data-testid=grid-row] td:first-child"),
            "Борис Иванов"
        );

        // 5. фильтры комбинируются: навык rust и стаж от 5 лет -> только Анна
        replace_text(&tab, "[data-testid=grid-filter-skills]", "rust");
        wait_count(&tab, "rust", ROWS, 2);
        type_into(&tab, "[data-testid=grid-filter-experience_years]", "5");
        wait_count(&tab, "rust and 5+ years", ROWS, 1);
        assert_eq!(
            text_of(&tab, "tr[data-testid=grid-row] td:first-child"),
            "Анна Смирнова"
        );
        assert!(marker_alive(&tab));

        // 6. строка ведёт в карточку
        wait_until(
            &tab,
            "row links to the card",
            "document.querySelector('[data-testid=grid-row] [data-testid=row-edit]').getAttribute('href').startsWith('/candidates/')",
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
            text_of(&tab, "tr[data-testid=grid-row] td:first-child"),
            "Иван Петров"
        );
        assert!(
            eval_str(&tab, "document.querySelector('[data-testid=grid-row]').innerText")
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

// ---------- карточка ----------

fn main_text(tab: &Tab) -> String {
    eval_str(tab, "document.querySelector('main').innerText")
}

async fn stack_with_ivan() -> (Stack, String) {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    stack.add_user(&owner, VIEWER.0, VIEWER.1, "viewer").await;
    let res = owner
        .post(
            "/api/persons",
            json!({"full_name": "Иван Петров", "email": "ivan@mail.ru", "phone": "+7 916 123 45 67",
                   "skills": ["Rust"], "declared_experience_years": 3, "source": "hh.ru"}),
        )
        .await;
    assert_eq!(res.status(), 201);
    let id = res.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    (stack, id)
}

#[tokio::test]
async fn card_sections_are_edited_independently() {
    let (stack, id) = stack_with_ivan().await;
    let base = stack.web_crm_url.clone();
    let card_id = id.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);

        // 1. из списка открывается карточка
        tab.navigate_to(&format!("{base}/candidates")).unwrap();
        wait_count(&tab, "row", ROWS, 1);
        click(&tab, "[data-testid=grid-row] [data-testid=row-edit]");
        wait_until(&tab, "card", &exists_js("[data-testid=profile-view]"));
        assert_eq!(path_of(&tab), format!("/candidates/{card_id}"));
        let text = main_text(&tab);
        for expected in [
            "Иван Петров",
            "+79161234567",
            "ivan@mail.ru",
            "Rust",
            "hh.ru",
            "3 года",
        ] {
            assert!(text.contains(expected), "card shows {expected}: {text}");
        }
        std::thread::sleep(Duration::from_millis(1500)); // гидратация
        set_marker(&tab);

        // 2. правка профиля и контактов; ошибка остаётся у поля
        click(&tab, "[data-testid=edit-profile]");
        wait_until(
            &tab,
            "profile form",
            &exists_js("[data-testid=profile-form]"),
        );
        set_value(
            &tab,
            "[data-testid=profile-form] input[name=email]",
            "broken",
        );
        click(&tab, "[data-testid=profile-save]");
        wait_until(&tab, "email error", &exists_js("[data-testid=error-email]"));
        wait_until(
            &tab,
            "form stays open",
            &exists_js("[data-testid=profile-form]"),
        );

        set_value(
            &tab,
            "[data-testid=profile-form] input[name=email]",
            "ivan.new@mail.ru",
        );
        set_value(
            &tab,
            "[data-testid=profile-form] input[name=phone]",
            "8 903 555 66 77",
        );
        set_value(
            &tab,
            "[data-testid=profile-form] input[name=skills]",
            "Rust, SQL, rust",
        );
        click(&tab, "[data-testid=profile-save]");
        wait_until(
            &tab,
            "view mode again",
            &exists_js("[data-testid=profile-view]"),
        );
        let text = main_text(&tab);
        for expected in ["+79035556677", "ivan.new@mail.ru", "Rust, SQL"] {
            assert!(
                text.contains(expected),
                "updated card shows {expected}: {text}"
            );
        }
        assert!(marker_alive(&tab), "saving must not reload the page");

        // 3. опыт: два периода пересчитывают стаж (2 г. 7 мес. + 1 г. 8 мес. = 4 года)
        click(&tab, "[data-testid=edit-employments]");
        wait_until(
            &tab,
            "employment editor",
            &exists_js("[data-testid=employments-form]"),
        );
        let row = "[data-testid=employment-row]";
        click(&tab, "[data-testid=add-employment]");
        set_row_value(&tab, row, 0, "company", "Acme");
        set_row_value(&tab, row, 0, "position", "Разработчик");
        set_row_value(&tab, row, 0, "start", "2020-01-01");
        set_row_value(&tab, row, 0, "end", "2022-08-01");
        click(&tab, "[data-testid=add-employment]");
        set_row_value(&tab, row, 1, "company", "Beta");
        set_row_value(&tab, row, 1, "position", "Тимлид");
        set_row_value(&tab, row, 1, "start", "2023-01-01");
        set_row_value(&tab, row, 1, "end", "2024-09-01");
        click(&tab, "[data-testid=employments-save]");
        wait_until(
            &tab,
            "employments saved",
            &exists_js("[data-testid=employments-view]"),
        );
        let text = main_text(&tab);
        assert!(
            text.contains("Acme") && text.contains("Beta") && text.contains("Тимлид"),
            "{text}"
        );
        wait_until(
            &tab,
            "experience recalculated",
            "document.querySelector('[data-testid=experience]').innerText.includes('4 года')",
        );
        assert!(marker_alive(&tab));

        // невалидный период не сохраняется, ошибка рядом
        click(&tab, "[data-testid=edit-employments]");
        wait_until(
            &tab,
            "editor again",
            &exists_js("[data-testid=employments-form]"),
        );
        set_row_value(&tab, row, 0, "end", "2019-01-01");
        click(&tab, "[data-testid=employments-save]");
        wait_until(
            &tab,
            "period error",
            &exists_js("[data-testid=error-employments]"),
        );
        click(&tab, "[data-testid=employments-cancel]");
        wait_until(
            &tab,
            "view after cancel",
            &exists_js("[data-testid=employments-view]"),
        );
        assert!(main_text(&tab).contains("Acme"), "previous list is intact");

        // 4. образование правится независимо от опыта
        click(&tab, "[data-testid=edit-educations]");
        wait_until(
            &tab,
            "education editor",
            &exists_js("[data-testid=educations-form]"),
        );
        click(&tab, "[data-testid=add-education]");
        let edu = "[data-testid=education-row]";
        set_row_value(&tab, edu, 0, "institution", "МГУ");
        set_row_value(&tab, edu, 0, "specialty", "Прикладная математика");
        set_row_value(&tab, edu, 0, "year_from", "2010");
        set_row_value(&tab, edu, 0, "year_to", "2015");
        click(&tab, "[data-testid=educations-save]");
        wait_until(
            &tab,
            "educations saved",
            &exists_js("[data-testid=educations-view]"),
        );
        let text = main_text(&tab);
        assert!(
            text.contains("МГУ") && text.contains("Прикладная математика"),
            "{text}"
        );
        assert!(
            text.contains("Acme"),
            "work history untouched by the education edit"
        );
    })
    .await
    .unwrap();

    // viewer видит карточку, но не может её менять
    let base = stack.web_crm_url.clone();
    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", VIEWER.0, VIEWER.1);
        tab.navigate_to(&format!("{base}/candidates/{id}")).unwrap();
        wait_until(
            &tab,
            "card for the viewer",
            &exists_js("[data-testid=profile-view]"),
        );
        for edit in ["edit-profile", "edit-employments", "edit-educations"] {
            assert!(
                eval(&tab, &gone_js(&format!("[data-testid={edit}]")))
                    == serde_json::Value::Bool(true),
                "viewer must not see {edit}"
            );
        }

        // неизвестный кандидат
        tab.navigate_to(&format!(
            "{base}/candidates/00000000-0000-0000-0000-000000000000"
        ))
        .unwrap();
        wait_until(
            &tab,
            "not found",
            "document.querySelector('main').innerText.includes('Кандидат не найден')",
        );
    })
    .await
    .unwrap();

    stack.stop().await;
}

// ---------- удаление ----------

const RECRUITER: (&str, &str) = ("rec@acme.io", "recruiter-pass-1");

#[tokio::test]
async fn only_the_owner_deletes_and_only_after_confirmation() {
    let (stack, id) = stack_with_ivan().await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    stack
        .add_user(&owner, RECRUITER.0, RECRUITER.1, "recruiter")
        .await;
    let base = stack.web_crm_url.clone();
    let card_id = id.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        tab.navigate_to(&format!("{base}/candidates/{card_id}"))
            .unwrap();
        wait_until(&tab, "card", &exists_js("[data-testid=delete]"));
        std::thread::sleep(Duration::from_millis(1500)); // гидратация
        set_marker(&tab);

        // без подтверждения ничего не удаляется
        click(&tab, "[data-testid=delete]");
        wait_until(
            &tab,
            "confirmation",
            &exists_js("[data-testid=delete-confirm]"),
        );
        assert!(main_text(&tab).contains("безвозвратно"));
        click(&tab, "[data-testid=delete-cancel]");
        wait_until(
            &tab,
            "confirmation closed",
            &gone_js("[data-testid=delete-confirm]"),
        );
        assert_eq!(path_of(&tab), format!("/candidates/{card_id}"));
        tab.navigate_to(&format!("{base}/candidates")).unwrap();
        wait_count(&tab, "candidate is still there", ROWS, 1);

        // подтверждённое удаление убирает кандидата и возвращает к списку
        click(&tab, "[data-testid=grid-row] [data-testid=row-edit]");
        wait_until(&tab, "card again", &exists_js("[data-testid=delete]"));
        std::thread::sleep(Duration::from_millis(1500));
        set_marker(&tab);
        click(&tab, "[data-testid=delete]");
        wait_until(
            &tab,
            "confirmation",
            &exists_js("[data-testid=delete-confirm]"),
        );
        click(&tab, "[data-testid=delete-confirm]");
        wait_until(
            &tab,
            "back to the list",
            "location.pathname === '/candidates'",
        );
        wait_until(
            &tab,
            "empty list",
            "document.querySelector('main').innerText.includes('Кандидатов пока нет')",
        );
        assert!(marker_alive(&tab), "deleting must not reload the page");
        tab.navigate_to(&format!("{base}/candidates/{card_id}"))
            .unwrap();
        wait_until(
            &tab,
            "gone",
            "document.querySelector('main').innerText.includes('Кандидат не найден')",
        );
    })
    .await
    .unwrap();

    // рекрутер может править, но не удалять
    let owner_again = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    let res = owner_again
        .post("/api/persons", json!({"full_name": "Пётр Сидоров"}))
        .await;
    let other_id = res.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let base = stack.web_crm_url.clone();
    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", RECRUITER.0, RECRUITER.1);
        tab.navigate_to(&format!("{base}/candidates/{other_id}"))
            .unwrap();
        wait_until(
            &tab,
            "card for the recruiter",
            &exists_js("[data-testid=edit-profile]"),
        );
        assert!(
            eval(&tab, &gone_js("[data-testid=delete]")) == serde_json::Value::Bool(true),
            "recruiter has no delete button"
        );
    })
    .await
    .unwrap();

    stack.stop().await;
}
