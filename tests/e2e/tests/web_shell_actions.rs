//! Кнопки быстрых действий в шапке: «+ Добавить», «Разбор», «?».
mod common;

use std::time::Duration;

use common::*;
use e2e::Stack;

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-password-1");

async fn seed() -> Stack {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    stack.add_user(&owner, VIEWER.0, VIEWER.1, "viewer").await;
    stack
}

fn sign_in(tab: &Tab, base: &str, user: (&str, &str)) {
    login_crm(tab, base, "acme", user.0, user.1);
    std::thread::sleep(Duration::from_millis(1500)); // гидратация
}

fn open_menu(tab: &Tab) {
    click(tab, "[data-testid=add-button]");
    wait_until(tab, "menu", &exists_js("[data-testid=add-menu]"));
}

#[tokio::test]
async fn the_add_menu_leads_to_each_form_and_closes_in_every_way() {
    let stack = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);
        set_marker(&tab);

        // три пункта, подсказка клавиши импорта
        open_menu(&tab);
        assert_eq!(eval(&tab, &count_js("[data-testid=add-menu] a")), 3);
        assert_eq!(
            eval_str(
                &tab,
                "document.querySelector('[data-testid=add-button]').getAttribute('aria-expanded')"
            ),
            "true"
        );
        assert!(text_of(&tab, "[data-testid=add-import]").contains('I'));

        // Esc закрывает меню
        tab.press_key("Escape").unwrap();
        wait_until(&tab, "closed by Esc", &gone_js("[data-testid=add-menu]"));

        // клик вне меню закрывает
        open_menu(&tab);
        click(&tab, "[data-testid=context-label]");
        wait_until(&tab, "closed by click", &gone_js("[data-testid=add-menu]"));

        // кандидат
        open_menu(&tab);
        click(&tab, "[data-testid=add-candidate]");
        wait_until(
            &tab,
            "new candidate",
            "location.pathname === '/candidates/new'",
        );
        wait_until(&tab, "menu closed", &gone_js("[data-testid=add-menu]"));

        // импорт
        open_menu(&tab);
        click(&tab, "[data-testid=add-import]");
        wait_until(&tab, "import", "location.pathname === '/candidates/import'");

        // вакансия: форма развёрнута сразу
        open_menu(&tab);
        click(&tab, "[data-testid=add-vacancy]");
        wait_until(&tab, "vacancies", "location.pathname === '/vacancies'");
        wait_until(
            &tab,
            "vacancy form",
            &exists_js("[data-testid=vacancy-form]"),
        );
        assert!(marker_alive(&tab), "no reload");
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn triage_and_cheatsheet_buttons_work() {
    let stack = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);

        click(&tab, "[data-testid=shell-cheat]");
        wait_until(&tab, "cheatsheet", &exists_js("[data-testid=cheatsheet]"));
        tab.press_key("Escape").unwrap();
        wait_until(&tab, "closed", &gone_js("[data-testid=cheatsheet]"));

        assert!(text_of(&tab, "[data-testid=shell-triage]").contains('T'));
        click(&tab, "[data-testid=shell-triage]");
        wait_until(&tab, "triage", "location.pathname === '/triage'");
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn the_new_vacancy_link_opens_the_form_and_the_palette_uses_it() {
    let stack = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);

        // прямая ссылка
        tab.navigate_to(&format!("{base}/vacancies?new=1")).unwrap();
        wait_until(&tab, "form", &exists_js("[data-testid=vacancy-form]"));

        // без параметра форма скрыта
        tab.navigate_to(&format!("{base}/vacancies")).unwrap();
        wait_until(&tab, "page", &exists_js("[data-testid=new-vacancy]"));
        std::thread::sleep(Duration::from_millis(800));
        assert_eq!(eval(&tab, &count_js("[data-testid=vacancy-form]")), 0);

        // команда палитры ведёт туда же
        tab.press_key_with_modifiers(
            "k",
            Some(&[headless_chrome::browser::tab::ModifierKey::Ctrl]),
        )
        .unwrap();
        wait_until(&tab, "palette", &exists_js("[data-testid=palette-input]"));
        tab.type_str("создать вак").unwrap();
        wait_until(
            &tab,
            "item",
            "document.querySelectorAll('[data-testid=palette-item]').length === 1",
        );
        tab.press_key("Enter").unwrap();
        wait_until(
            &tab,
            "form from palette",
            &exists_js("[data-testid=vacancy-form]"),
        );
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn a_viewer_sees_search_and_help_but_no_write_buttons() {
    let stack = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, VIEWER);

        assert_eq!(eval(&tab, &count_js("[data-testid=search-button]")), 1);
        assert_eq!(eval(&tab, &count_js("[data-testid=shell-cheat]")), 1);
        assert_eq!(eval(&tab, &count_js("[data-testid=add-button]")), 0);
        assert_eq!(eval(&tab, &count_js("[data-testid=shell-triage]")), 0);

        // форма вакансий по ссылке не раскрывается без права записи
        tab.navigate_to(&format!("{base}/vacancies?new=1")).unwrap();
        wait_until(&tab, "page", &exists_js("nav"));
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(eval(&tab, &count_js("[data-testid=vacancy-form]")), 0);
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn the_admin_app_has_no_quick_actions() {
    let stack = Stack::start_with_web().await;
    let base = stack.web_admin_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        tab.navigate_to(&format!("{base}/login")).unwrap();
        wait_until(&tab, "login", &exists_js("input[name=password]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=add-button]")), 0);
        assert_eq!(eval(&tab, &count_js("[data-testid=shell-cheat]")), 0);
    })
    .await
    .unwrap();
    stack.stop().await;
}
