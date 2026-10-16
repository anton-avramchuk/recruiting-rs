//! Экран «Безопасность»: управление пользователями организации в браузере.
mod common;

use std::time::Duration;

use common::*;
use e2e::Stack;
use headless_chrome::browser::tab::ModifierKey;

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-password-1");
const NEW: &str = "new@acme.io";

fn text(tab: &Tab, selector: &str) -> String {
    eval_str(
        tab,
        &format!("(document.querySelector({selector:?})||{{}}).innerText||''"),
    )
}

/// Выбирает в `select` вариант, текст которого содержит `label`, и сообщает фреймворку.
fn choose(tab: &Tab, selector: &str, label: &str) {
    eval(
        tab,
        &format!(
            "(() => {{ const s = document.querySelector({selector:?}); \
             const o = Array.from(s.options).find(o => o.textContent.includes({label:?})); \
             s.value = o.value; s.dispatchEvent(new Event('change', {{bubbles: true}})); }})()"
        ),
    );
}

fn row(email: &str, part: &str) -> String {
    format!("[data-email='{email}'] {part}")
}

async fn world() -> Stack {
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

fn open_user(tab: &Tab, email: &str) {
    click(tab, &format!("a[data-email='{email}']"));
    wait_until(
        tab,
        "user page",
        &exists_js("[data-testid=user-role-select]"),
    );
}

fn back_to_users(tab: &Tab) {
    click(tab, "[data-testid=back-to-users]");
    wait_until(tab, "users list", &exists_js("[data-testid=users]"));
}

fn error_contains(part: &str) -> String {
    format!(
        "(document.querySelector('[data-testid=users-error]')||{{}}).innerText?.includes({part:?})"
    )
}

fn ok_contains(part: &str) -> String {
    format!(
        "(document.querySelector('[data-testid=users-ok]')||{{}}).innerText?.includes({part:?})"
    )
}

#[tokio::test]
async fn the_owner_manages_users_end_to_end() {
    let stack = world().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);
        set_marker(&tab);

        click(&tab, "nav a[href='/security']");
        wait_until(&tab, "security page", &exists_js("[data-testid=user-new]"));
        wait_count(&tab, "two users", "[data-testid=user-row]", 2);
        assert_eq!(eval(&tab, &count_js("[data-testid=security-tabs]")), 1);

        // создание на отдельной странице
        click(&tab, "[data-testid=user-new]");
        wait_until(&tab, "new user page", &exists_js("[data-testid=user-email]"));
        assert_eq!(path_of(&tab), "/security/users/new");
        type_into(&tab, "[data-testid=user-email]", NEW);
        type_into(&tab, "[data-testid=user-name]", "Новый");
        type_into(&tab, "[data-testid=user-password]", "new-password-1");
        click(&tab, "[data-testid=user-add]");
        wait_until(&tab, "back on list", &exists_js("[data-testid=users]"));
        wait_count(&tab, "three users", "[data-testid=user-row]", 3);
        assert!(text(&tab, &row(NEW, "[data-testid=user-title]")).contains("Новый"));

        // занятый email и слабый пароль: ошибка у формы, остаёмся на странице
        click(&tab, "[data-testid=user-new]");
        wait_until(&tab, "new user page", &exists_js("[data-testid=user-email]"));
        type_into(&tab, "[data-testid=user-email]", NEW);
        type_into(&tab, "[data-testid=user-password]", "new-password-1");
        click(&tab, "[data-testid=user-add]");
        wait_until(&tab, "duplicate error", &error_contains("уже есть"));
        replace_text(&tab, "[data-testid=user-email]", "weak@acme.io");
        replace_text(&tab, "[data-testid=user-password]", "short");
        click(&tab, "[data-testid=user-add]");
        wait_until(&tab, "weak password error", &error_contains("8"));
        assert_eq!(path_of(&tab), "/security/users/new");
        back_to_users(&tab);
        assert_eq!(eval(&tab, &count_js("[data-testid=user-row]")), 3);

        // смена роли на странице пользователя
        open_user(&tab, NEW);
        choose(&tab, "[data-testid=user-role-select]", "Наблюдатель");
        click(&tab, "[data-testid=user-role-save]");
        wait_until(&tab, "role saved", &ok_contains("Роль"));
        wait_until(
            &tab,
            "heading shows the role",
            "document.querySelector('[data-testid=user-heading]').innerText.includes('Наблюдатель')",
        );
        back_to_users(&tab);
        assert_eq!(
            text(&tab, &row(NEW, "[data-testid=user-role-label]")),
            "Наблюдатель"
        );

        // единственный владелец не может понизить или заблокировать себя
        open_user(&tab, OWNER.0);
        choose(&tab, "[data-testid=user-role-select]", "Рекрутёр");
        click(&tab, "[data-testid=user-role-save]");
        wait_until(&tab, "owner guard", &error_contains("владелец"));
        wait_until(
            &tab,
            "owner role restored",
            "document.querySelector('[data-testid=user-role-select]').value === 'owner'",
        );
        click(&tab, "[data-testid=user-toggle]");
        wait_until(&tab, "self block refused", &error_contains("самого себя"));
        assert_eq!(text(&tab, "[data-testid=user-status]"), "Активен");
        back_to_users(&tab);

        // блокировка и разблокировка
        open_user(&tab, NEW);
        click(&tab, "[data-testid=user-toggle]");
        wait_until(
            &tab,
            "blocked",
            "(document.querySelector('[data-testid=user-status]')||{}).innerText === 'Заблокирован'",
        );
        click(&tab, "[data-testid=user-toggle]");
        wait_until(
            &tab,
            "unblocked",
            "(document.querySelector('[data-testid=user-status]')||{}).innerText === 'Активен'",
        );

        // смена имени
        replace_text(&tab, "[data-testid=user-name-input]", "Другое имя");
        click(&tab, "[data-testid=user-name-save]");
        wait_until(&tab, "name saved", &ok_contains("Имя"));

        // сброс пароля: слабый, затем нормальный
        type_into(&tab, "[data-testid=password-input]", "short");
        click(&tab, "[data-testid=password-save]");
        wait_until(&tab, "weak reset refused", &error_contains("8"));
        replace_text(&tab, "[data-testid=password-input]", "fresh-password-1");
        click(&tab, "[data-testid=password-save]");
        wait_until(&tab, "password changed", &ok_contains("Пароль изменён"));
        back_to_users(&tab);
        assert!(text(&tab, &row(NEW, "[data-testid=user-title]")).contains("Другое имя"));

        // владельцу палитра предлагает раздел
        tab.press_key_with_modifiers("k", Some(&[ModifierKey::Ctrl]))
            .unwrap();
        wait_until(&tab, "palette", &exists_js("[data-testid=palette-input]"));
        type_into(&tab, "[data-testid=palette-input]", "безопас");
        wait_until(&tab, "security offered", &exists_js("[data-testid=palette-item]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=palette-item]")), 1);

        // новый пароль работает в отдельном браузере; наблюдатель не видит раздел
        let other = launch();
        let tab2 = other.new_tab().unwrap();
        login_crm(&tab2, &base, "acme", NEW, "fresh-password-1");
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(eval(&tab2, &count_js("nav a[href='/security']")), 0);
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn other_roles_do_not_see_the_security_section() {
    let stack = world().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, VIEWER);
        assert_eq!(eval(&tab, &count_js("nav a[href='/security']")), 0);

        tab.navigate_to(&format!("{base}/security")).unwrap();
        wait_until(&tab, "denied", &exists_js("[data-testid=security-denied]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=user-row]")), 0);

        // палитра не предлагает раздел
        std::thread::sleep(Duration::from_millis(1500)); // гидратация после перезагрузки
        tab.press_key_with_modifiers("k", Some(&[ModifierKey::Ctrl]))
            .unwrap();
        wait_until(&tab, "palette", &exists_js("[data-testid=palette-input]"));
        type_into(&tab, "[data-testid=palette-input]", "безопас");
        wait_until(
            &tab,
            "nothing offered",
            &exists_js("[data-testid=palette-empty]"),
        );
    })
    .await
    .unwrap();
    stack.stop().await;
}
