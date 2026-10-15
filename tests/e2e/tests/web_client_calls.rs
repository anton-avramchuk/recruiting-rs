//! Браузер ходит в BFF напрямую; первая отрисовка содержит данные; истёкшая сессия ведёт на вход.
mod common;

use std::time::Duration;

use common::*;
use e2e::Stack;
use headless_chrome::protocol::cdp::Network::DeleteCookies;
use serde_json::{Value, json};

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");

async fn stack_with_vacancy() -> Stack {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    let client = stack.add_client(&owner, "Acme").await;
    let res = owner
        .post(
            "/api/vacancies",
            json!({"title": "Senior Go", "client_id": client}),
        )
        .await;
    assert_eq!(res.status(), 201);
    stack
}

/// Адреса запросов fetch/XHR, которые страница отправила с момента загрузки.
fn fetched_urls(tab: &Tab) -> Vec<String> {
    let value = eval(
        tab,
        "JSON.stringify(performance.getEntriesByType('resource') \
         .filter(e => e.initiatorType === 'fetch' || e.initiatorType === 'xmlhttprequest') \
         .map(e => e.name))",
    );
    serde_json::from_str(value.as_str().unwrap_or("[]")).unwrap_or_default()
}

#[tokio::test]
async fn data_requests_go_straight_to_the_bff() {
    let stack = stack_with_vacancy().await;
    let base = stack.web_crm_url.clone();
    let bff = stack.bff_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);

        // вход тоже ушёл напрямую в BFF
        let urls = fetched_urls(&tab);
        assert!(
            urls.iter().any(|u| u == &format!("{bff}/bff/login")),
            "login goes to the BFF: {urls:?}"
        );

        // клиентская навигация: данные запрашиваются на публичном адресе BFF
        click(&tab, "nav a[href='/vacancies']");
        wait_until(&tab, "board", &exists_js("[data-testid=vacancy-select]"));
        std::thread::sleep(Duration::from_millis(500));
        let urls = fetched_urls(&tab);
        assert!(
            urls.iter()
                .any(|u| u.starts_with(&format!("{bff}/api/vacancies"))),
            "data requests go to the BFF: {urls:?}"
        );
        assert!(
            !urls.iter().any(|u| u.starts_with(&base)),
            "nothing goes through the frontend server: {urls:?}"
        );
        assert!(
            !urls.iter().any(|u| u.contains("/api/list_vacancies")),
            "no server function calls: {urls:?}"
        );

        // выход тоже из браузера
        click(&tab, "header button[type=submit]");
        wait_until(&tab, "login", "location.pathname === '/login'");
        let urls = fetched_urls(&tab);
        assert!(
            urls.iter().any(|u| u == &format!("{bff}/bff/logout")),
            "{urls:?}"
        );
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn first_render_already_contains_the_data() {
    let stack = stack_with_vacancy().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);

        // тот же запрос страницы, что делает браузер при открытии адреса: HTML с сервера
        let html = tab
            .evaluate(
                &format!(
                    "fetch('{base}/vacancies', {{credentials: 'include'}}).then(r => r.text())"
                ),
                true,
            )
            .unwrap()
            .value
            .unwrap_or(Value::Null);
        let html = html.as_str().unwrap_or_default();
        assert!(
            html.contains("Senior Go"),
            "the server rendered the data without the browser's help"
        );
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn an_expired_session_leads_back_to_the_login() {
    let stack = stack_with_vacancy().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        tab.navigate_to(&format!("{base}/applications")).unwrap();
        wait_until(&tab, "table page", &exists_js("[data-testid=filter-stage]"));
        std::thread::sleep(Duration::from_millis(1500));

        // сессия пропала (cookie удалена): следующий клиентский запрос получает 401
        for cookie in tab.get_cookies().unwrap() {
            if cookie.name == "crm_session" {
                tab.call_method(DeleteCookies {
                    name: cookie.name.clone(),
                    url: None,
                    domain: Some(cookie.domain.clone()),
                    path: Some(cookie.path.clone()),
                    partition_key: None,
                })
                .unwrap();
            }
        }
        eval(
            &tab,
            "(() => { const el = document.querySelector('[data-testid=filter-stage]'); el.value = 'hr'; \
             el.dispatchEvent(new Event('change', {bubbles: true})); })()",
        );
        wait_until(&tab, "back to login", "location.pathname === '/login'");
    })
    .await
    .unwrap();
    stack.stop().await;
}
