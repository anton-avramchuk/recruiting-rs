//! Переключение светлой и тёмной темы в обоих приложениях.
mod common;

use std::time::Duration;

use common::*;
use e2e::{ADMIN_EMAIL, ADMIN_PASSWORD, Stack};
use headless_chrome::browser::tab::ModifierKey;
use headless_chrome::protocol::cdp::Emulation::{MediaFeature, SetEmulatedMedia};

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");
const LIGHT_BG: &str = "rgb(243, 239, 230)";
const DARK_BG: &str = "rgb(21, 20, 15)";

fn background(tab: &Tab) -> String {
    eval_str(tab, "getComputedStyle(document.body).backgroundColor")
}

fn chosen(tab: &Tab) -> String {
    eval_str(tab, "document.documentElement.dataset.theme || ''")
}

fn stored(tab: &Tab) -> String {
    eval_str(tab, "localStorage.getItem('theme') || ''")
}

fn system_dark(tab: &Tab, dark: bool) {
    tab.call_method(SetEmulatedMedia {
        media: None,
        features: Some(vec![MediaFeature {
            name: "prefers-color-scheme".into(),
            value: if dark { "dark" } else { "light" }.into(),
        }]),
    })
    .unwrap();
}

fn reload(tab: &Tab) {
    tab.reload(false, None).unwrap();
    wait_until(tab, "shell", &exists_js("[data-testid=theme-button]"));
}

#[tokio::test]
async fn the_button_switches_the_theme_and_the_choice_survives_a_reload() {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        system_dark(&tab, false);
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        std::thread::sleep(Duration::from_millis(1500)); // гидратация
        set_marker(&tab);
        assert_eq!(background(&tab), LIGHT_BG);
        assert_eq!(chosen(&tab), "");

        // ранний скрипт на месте
        assert_eq!(
            eval(&tab, "Array.from(document.scripts).some(s => s.textContent.includes('getItem(\"theme\")'))"),
            true
        );

        // свет -> тьма без перезагрузки
        click(&tab, "[data-testid=theme-button]");
        wait_until(&tab, "dark", "document.documentElement.dataset.theme === 'dark'");
        assert_eq!(background(&tab), DARK_BG);
        assert_eq!(stored(&tab), "dark");
        assert_eq!(
            eval_str(&tab, "document.querySelector('[data-testid=theme-button]').getAttribute('aria-pressed')"),
            "true"
        );
        assert!(marker_alive(&tab), "no reload");

        // после перезагрузки тема на месте
        reload(&tab);
        assert_eq!(chosen(&tab), "dark");
        assert_eq!(background(&tab), DARK_BG);

        // обратно
        std::thread::sleep(Duration::from_millis(1500));
        click(&tab, "[data-testid=theme-button]");
        wait_until(&tab, "light", "document.documentElement.dataset.theme === 'light'");
        assert_eq!(background(&tab), LIGHT_BG);
        assert_eq!(stored(&tab), "light");
        reload(&tab);
        assert_eq!(background(&tab), LIGHT_BG);
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn the_system_theme_applies_until_the_user_chooses() {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        system_dark(&tab, true);
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        std::thread::sleep(Duration::from_millis(1500));

        // выбора нет: действует тема системы
        assert_eq!(chosen(&tab), "");
        assert_eq!(background(&tab), DARK_BG);

        // система меняется на лету
        system_dark(&tab, false);
        wait_until(
            &tab,
            "follows the system",
            &format!("getComputedStyle(document.body).backgroundColor === '{LIGHT_BG}'"),
        );
        system_dark(&tab, true);
        wait_until(
            &tab,
            "dark again",
            &format!("getComputedStyle(document.body).backgroundColor === '{DARK_BG}'"),
        );

        // кнопка при тёмной системе делает светлую, и явный выбор сильнее системы
        click(&tab, "[data-testid=theme-button]");
        wait_until(
            &tab,
            "light",
            "document.documentElement.dataset.theme === 'light'",
        );
        assert_eq!(background(&tab), LIGHT_BG);
        reload(&tab);
        assert_eq!(
            background(&tab),
            LIGHT_BG,
            "explicit light wins over a dark system"
        );
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn the_palette_command_toggles_the_theme_and_a_broken_storage_is_harmless() {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        system_dark(&tab, false);
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        std::thread::sleep(Duration::from_millis(1500));

        tab.press_key_with_modifiers("k", Some(&[ModifierKey::Ctrl]))
            .unwrap();
        wait_until(&tab, "palette", &exists_js("[data-testid=palette-input]"));
        tab.type_str("тему").unwrap();
        wait_until(
            &tab,
            "item",
            "document.querySelectorAll('[data-testid=palette-item]').length === 1",
        );
        tab.press_key("Enter").unwrap();
        wait_until(
            &tab,
            "dark",
            "document.documentElement.dataset.theme === 'dark'",
        );
        wait_until(&tab, "palette closed", &gone_js("[data-testid=palette]"));
        assert_eq!(background(&tab), DARK_BG);

        // хранилище бросает исключения: тема всё равно меняется
        eval(
            &tab,
            "Storage.prototype.setItem = function () { throw new Error('quota'); }",
        );
        click(&tab, "[data-testid=theme-button]");
        wait_until(
            &tab,
            "light",
            "document.documentElement.dataset.theme === 'light'",
        );
        assert_eq!(background(&tab), LIGHT_BG);
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn the_admin_app_has_the_same_button() {
    let stack = Stack::start_with_web().await;
    let base = stack.web_admin_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        system_dark(&tab, false);
        tab.navigate_to(&format!("{base}/login")).unwrap();
        wait_until(&tab, "login", &exists_js("input[name=password]"));
        std::thread::sleep(Duration::from_millis(1500));
        type_into(&tab, "input[name=email]", ADMIN_EMAIL);
        type_into(&tab, "input[name=password]", ADMIN_PASSWORD);
        click(&tab, "button[type=submit]");
        wait_until(&tab, "shell", &exists_js("[data-testid=theme-button]"));
        std::thread::sleep(Duration::from_millis(1500));

        assert_eq!(background(&tab), LIGHT_BG);
        click(&tab, "[data-testid=theme-button]");
        wait_until(
            &tab,
            "dark",
            "document.documentElement.dataset.theme === 'dark'",
        );
        assert_eq!(background(&tab), DARK_BG);
        reload(&tab);
        assert_eq!(background(&tab), DARK_BG);
    })
    .await
    .unwrap();
    stack.stop().await;
}
