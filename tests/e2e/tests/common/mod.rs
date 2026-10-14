#![allow(dead_code)]
//! Общие помощники браузерных тестов (headless Chrome).
use std::time::{Duration, Instant};

use headless_chrome::LaunchOptions;
pub use headless_chrome::{Browser, Tab};
use serde_json::Value;

pub fn launch() -> Browser {
    Browser::new(LaunchOptions {
        headless: true,
        window_size: Some((1280, 900)),
        idle_browser_timeout: Duration::from_secs(300),
        ..LaunchOptions::default()
    })
    .expect("launch Chrome (install Google Chrome)")
}

pub fn eval(tab: &Tab, js: &str) -> Value {
    tab.evaluate(js, false)
        .expect("evaluate")
        .value
        .unwrap_or(Value::Null)
}

pub fn eval_str(tab: &Tab, js: &str) -> String {
    eval(tab, js).as_str().unwrap_or_default().to_string()
}

/// Ждёт, пока выражение JS станет истинным; при таймауте печатает адрес и текст страницы.
pub fn wait_until(tab: &Tab, what: &str, js: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if eval(tab, js) == Value::Bool(true) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let url = tab.get_url();
    let body = eval(tab, "document.body.innerText");
    panic!("timeout waiting for {what}; url={url}; body={body}");
}

pub fn type_into(tab: &Tab, selector: &str, text: &str) {
    tab.wait_for_element(selector)
        .unwrap_or_else(|e| panic!("{selector}: {e}"))
        .click()
        .unwrap();
    tab.type_str(text).unwrap();
}

/// Очищает поле и вводит значение (поле остаётся управляемым фреймворком).
pub fn replace_text(tab: &Tab, selector: &str, text: &str) {
    let js = format!(
        "(() => {{ const el = document.querySelector({selector:?}); el.focus(); el.select(); }})()"
    );
    tab.wait_for_element(selector)
        .unwrap_or_else(|e| panic!("{selector}: {e}"));
    eval(tab, &js);
    tab.press_key("Delete").unwrap();
    tab.type_str(text).unwrap();
}

pub fn text_of(tab: &Tab, selector: &str) -> String {
    eval_str(
        tab,
        &format!("(document.querySelector({selector:?})||{{}}).textContent||''"),
    )
}

pub fn path_of(tab: &Tab) -> String {
    eval_str(tab, "location.pathname + location.search")
}

/// Метка в `window`: исчезает при полной перезагрузке страницы.
pub fn set_marker(tab: &Tab) {
    eval(tab, "window.__marker = 'alive'");
}

pub fn marker_alive(tab: &Tab) -> bool {
    eval(tab, "window.__marker === 'alive'") == Value::Bool(true)
}

pub fn click(tab: &Tab, selector: &str) {
    tab.wait_for_element(selector)
        .unwrap_or_else(|e| panic!("{selector}: {e}"))
        .click()
        .unwrap();
}

pub fn exists_js(selector: &str) -> String {
    format!("!!document.querySelector({selector:?})")
}

pub fn gone_js(selector: &str) -> String {
    format!("!document.querySelector({selector:?})")
}

pub fn count_js(selector: &str) -> String {
    format!("document.querySelectorAll({selector:?}).length")
}

/// Ждёт, пока количество элементов станет равным `n`.
pub fn wait_count(tab: &Tab, what: &str, selector: &str, n: usize) {
    wait_until(tab, what, &format!("{} === {n}", count_js(selector)));
}

/// Вход в клиентское приложение через форму.
pub fn login_crm(tab: &Tab, base: &str, tenant: &str, email: &str, password: &str) {
    tab.navigate_to(&format!("{base}/login")).unwrap();
    wait_until(tab, "login form", &exists_js("input[name=password]"));
    std::thread::sleep(Duration::from_millis(1500)); // гидратация
    type_into(tab, "input[name=tenant]", tenant);
    type_into(tab, "input[name=email]", email);
    type_into(tab, "input[name=password]", password);
    click(tab, "button[type=submit]");
    wait_until(tab, "app shell", &exists_js("[data-testid=user-label]"));
}
