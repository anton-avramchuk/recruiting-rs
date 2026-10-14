//! Админское приложение в настоящем браузере: вход, список организаций, создание, приостановка, фильтр.
mod common;

use std::time::Duration;

use common::*;
use e2e::{ADMIN_EMAIL, ADMIN_PASSWORD, Stack};

fn row_exists(slug: &str) -> String {
    format!("!!document.querySelector(\"tr[data-slug='{slug}']\")")
}

fn row_text(slug: &str) -> String {
    format!("(document.querySelector(\"tr[data-slug='{slug}']\")||{{}}).innerText||''")
}

#[tokio::test]
async fn platform_admin_manages_organizations() {
    let stack = Stack::start_with_web().await;
    let base = stack.web_admin_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();

        // 1. вход администратора (без поля «Организация»)
        tab.navigate_to(&base).unwrap();
        wait_until(
            &tab,
            "login form",
            "!!document.querySelector('input[name=password]')",
        );
        assert_eq!(eval(&tab, "location.pathname"), "/login");
        assert!(
            eval(
                &tab,
                "document.querySelector('input[name=tenant]:not([type=hidden])')"
            )
            .is_null()
        );
        std::thread::sleep(Duration::from_millis(1500)); // гидратация
        type_into(&tab, "input[name=email]", ADMIN_EMAIL);
        type_into(&tab, "input[name=password]", ADMIN_PASSWORD);
        tab.find_element("button[type=submit]")
            .unwrap()
            .click()
            .unwrap();
        wait_until(
            &tab,
            "tenants page",
            "!!document.querySelector('[data-testid=search]')",
        );
        assert_eq!(eval(&tab, "location.pathname"), "/");
        assert!(
            eval(&tab, "document.querySelector('main').innerText")
                .as_str()
                .unwrap()
                .contains("Организаций не найдено")
        );

        // 2. создание организации из формы
        eval(&tab, "window.__marker = 'alive'");
        type_into(&tab, "input[name=slug]", "acme");
        type_into(&tab, "input[name=name]", "Acme Inc");
        type_into(&tab, "input[name=owner_email]", "boss@acme.io");
        type_into(&tab, "input[name=owner_password]", "boss-password-1");
        tab.find_element("form.card button[type=submit]")
            .unwrap()
            .click()
            .unwrap();
        wait_until(&tab, "created row", &row_exists("acme"));
        assert!(
            eval(&tab, &row_text("acme"))
                .as_str()
                .unwrap()
                .contains("Активна"),
            "new organization is active"
        );
        assert!(
            eval(&tab, "document.body.innerText")
                .as_str()
                .unwrap()
                .contains("Организация создана.")
        );

        // повторный slug даёт понятную ошибку
        type_into(&tab, "input[name=slug]", "acme");
        type_into(&tab, "input[name=name]", "Duplicate");
        type_into(&tab, "input[name=owner_email]", "other@acme.io");
        type_into(&tab, "input[name=owner_password]", "other-password-1");
        tab.find_element("form.card button[type=submit]")
            .unwrap()
            .click()
            .unwrap();
        wait_until(
            &tab,
            "duplicate error",
            "document.body.innerText.includes('Такой адрес уже занят')",
        );

        // 3. приостановка: статус меняется на месте, страница не перезагружается
        tab.find_element("tr[data-slug='acme'] button")
            .unwrap()
            .click()
            .unwrap();
        wait_until(
            &tab,
            "suspended badge",
            &format!("({}).includes('Приостановлена')", row_text("acme")),
        );
        assert_eq!(eval(&tab, "window.__marker"), "alive", "no page reload");
        assert!(
            eval(&tab, &row_text("acme"))
                .as_str()
                .unwrap()
                .contains("Активировать"),
            "the action flips"
        );

        // 4. фильтр по статусу и поиск работают без перезагрузки
        eval(
            &tab,
            "(() => { const s = document.querySelector('[data-testid=status-filter]'); \
             s.value = 'active'; s.dispatchEvent(new Event('change', {bubbles: true})); })()",
        );
        wait_until(
            &tab,
            "active filter hides the suspended row",
            &format!("!({})", row_exists("acme")),
        );
        eval(
            &tab,
            "(() => { const s = document.querySelector('[data-testid=status-filter]'); \
             s.value = 'suspended'; s.dispatchEvent(new Event('change', {bubbles: true})); })()",
        );
        wait_until(&tab, "suspended filter shows the row", &row_exists("acme"));
        type_into(&tab, "[data-testid=search]", "zzz-nothing");
        wait_until(
            &tab,
            "search finds nothing",
            &format!("!({})", row_exists("acme")),
        );
        assert_eq!(eval(&tab, "window.__marker"), "alive");

        // 5. активация возвращает статус
        eval(
            &tab,
            "(() => { const i = document.querySelector('[data-testid=search]'); \
             i.value = ''; i.dispatchEvent(new Event('input', {bubbles: true})); })()",
        );
        wait_until(&tab, "row is back", &row_exists("acme"));
        tab.find_element("tr[data-slug='acme'] button")
            .unwrap()
            .click()
            .unwrap();
        // фильтр «приостановленные» ещё включён: активированная организация уходит из списка
        wait_until(
            &tab,
            "activated row leaves the suspended list",
            &format!("!({})", row_exists("acme")),
        );
        eval(
            &tab,
            "(() => { const s = document.querySelector('[data-testid=status-filter]');              s.value = ''; s.dispatchEvent(new Event('change', {bubbles: true})); })()",
        );
        wait_until(
            &tab,
            "active badge",
            &format!("({}).includes('Активна')", row_text("acme")),
        );
        assert_eq!(eval(&tab, "window.__marker"), "alive");
    })
    .await
    .unwrap();

    stack.stop().await;
}
