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
        wait_until(&tab, "security page", &exists_js("[data-testid=user-email]"));
        wait_count(&tab, "two users", "[data-testid=user-row]", 2);

        // создание
        type_into(&tab, "[data-testid=user-email]", NEW);
        type_into(&tab, "[data-testid=user-name]", "Новый");
        type_into(&tab, "[data-testid=user-password]", "new-password-1");
        click(&tab, "[data-testid=user-add]");
        wait_until(&tab, "user added", &exists_js(&row(NEW, "")));
        assert!(text(&tab, "[data-testid=users-ok]").contains("добавлен"));
        assert_eq!(eval(&tab, &count_js("[data-testid=user-row]")), 3);

        // занятый email
        type_into(&tab, "[data-testid=user-email]", NEW);
        type_into(&tab, "[data-testid=user-password]", "new-password-1");
        click(&tab, "[data-testid=user-add]");
        wait_until(&tab, "duplicate error", &exists_js("[data-testid=users-error]"));
        assert!(text(&tab, "[data-testid=users-error]").contains("уже есть"));

        // слабый пароль: ошибка у формы, пользователь не создан
        replace_text(&tab, "[data-testid=user-email]", "weak@acme.io");
        replace_text(&tab, "[data-testid=user-password]", "short");
        click(&tab, "[data-testid=user-add]");
        wait_until(
            &tab,
            "weak password error",
            "(document.querySelector('[data-testid=users-error]')||{}).innerText?.includes('8')",
        );
        assert_eq!(eval(&tab, &count_js("[data-testid=user-row]")), 3);

        // смена роли
        choose(&tab, &row(NEW, "[data-testid=user-role-select]"), "Наблюдатель");
        wait_until(
            &tab,
            "role saved",
            &format!(
                "document.querySelector({:?}).value === 'viewer' && (document.querySelector('[data-testid=users-ok]')||{{}}).innerText?.includes('Роль')",
                row(NEW, "[data-testid=user-role-select]")
            ),
        );

        // единственный владелец не может понизить себя
        choose(&tab, &row(OWNER.0, "[data-testid=user-role-select]"), "Рекрутёр");
        wait_until(
            &tab,
            "owner guard",
            "(document.querySelector('[data-testid=users-error]')||{}).innerText?.includes('владелец')",
        );
        wait_until(
            &tab,
            "owner role restored",
            &format!(
                "document.querySelector({:?}).value === 'owner'",
                row(OWNER.0, "[data-testid=user-role-select]")
            ),
        );

        // и заблокировать себя
        click(&tab, &row(OWNER.0, "[data-testid=user-toggle]"));
        wait_until(
            &tab,
            "self block refused",
            "(document.querySelector('[data-testid=users-error]')||{}).innerText?.includes('самого себя')",
        );
        assert_eq!(text(&tab, &row(OWNER.0, "[data-testid=user-status]")), "Активен");

        // блокировка и разблокировка
        click(&tab, &row(NEW, "[data-testid=user-toggle]"));
        wait_until(
            &tab,
            "blocked",
            &format!(
                "(document.querySelector({:?})||{{}}).innerText === 'Заблокирован'",
                row(NEW, "[data-testid=user-status]")
            ),
        );
        click(&tab, &row(NEW, "[data-testid=user-toggle]"));
        wait_until(
            &tab,
            "unblocked",
            &format!(
                "(document.querySelector({:?})||{{}}).innerText === 'Активен'",
                row(NEW, "[data-testid=user-status]")
            ),
        );

        // сброс пароля: слабый, затем нормальный
        click(&tab, &row(NEW, "[data-testid=user-reset]"));
        wait_until(&tab, "reset form", &exists_js("[data-testid=password-input]"));
        type_into(&tab, "[data-testid=password-input]", "short");
        click(&tab, "[data-testid=password-save]");
        wait_until(
            &tab,
            "weak reset refused",
            "(document.querySelector('[data-testid=users-error]')||{}).innerText?.includes('8')",
        );
        wait_until(&tab, "reset form still open", &exists_js("[data-testid=password-input]"));
        replace_text(&tab, "[data-testid=password-input]", "fresh-password-1");
        click(&tab, "[data-testid=password-save]");
        wait_until(
            &tab,
            "password changed",
            "(document.querySelector('[data-testid=users-ok]')||{}).innerText?.includes('Пароль изменён')",
        );

        // владельцу палитра предлагает раздел
        tab.press_key_with_modifiers("k", Some(&[ModifierKey::Ctrl]))
            .unwrap();
        wait_until(&tab, "palette", &exists_js("[data-testid=palette-input]"));
        type_into(&tab, "[data-testid=palette-input]", "безопас");
        wait_until(&tab, "security offered", &exists_js("[data-testid=palette-item]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=palette-item]")), 1);

        // новый пароль работает в отдельном браузере; роль «Наблюдатель» не видит раздел
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
