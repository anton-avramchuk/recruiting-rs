//! Клиентское приложение в настоящем браузере (headless Chrome): вход, защита маршрутов, каркас.
use std::time::{Duration, Instant};

use e2e::Stack;
use headless_chrome::{Browser, LaunchOptions, Tab};

const OWNER_EMAIL: &str = "boss@acme.io";
const OWNER_PASSWORD: &str = "boss-password-1";

fn launch() -> Browser {
    Browser::new(LaunchOptions {
        headless: true,
        window_size: Some((1280, 900)),
        idle_browser_timeout: Duration::from_secs(300),
        ..LaunchOptions::default()
    })
    .expect("launch Chrome (install Google Chrome)")
}

fn eval(tab: &Tab, js: &str) -> serde_json::Value {
    tab.evaluate(js, false)
        .expect("evaluate")
        .value
        .unwrap_or(serde_json::Value::Null)
}

/// Ждёт, пока выражение JS станет истинным.
fn wait_until(tab: &Tab, what: &str, js: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if eval(tab, js) == serde_json::Value::Bool(true) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let url = tab.get_url();
    let body = eval(tab, "document.body.innerText");
    panic!("timeout waiting for {what}; url={url}; body={body}");
}

fn type_into(tab: &Tab, selector: &str, text: &str) {
    tab.wait_for_element(selector)
        .unwrap_or_else(|e| panic!("{selector}: {e}"))
        .click()
        .unwrap();
    tab.type_str(text).unwrap();
}

fn text_of(tab: &Tab, selector: &str) -> String {
    eval(
        tab,
        &format!("(document.querySelector('{selector}')||{{}}).textContent||''"),
    )
    .as_str()
    .unwrap_or_default()
    .to_string()
}

/// Метка в `window`: исчезает при полной перезагрузке страницы.
fn set_marker(tab: &Tab) {
    eval(tab, "window.__marker = 'alive'");
}

fn marker_alive(tab: &Tab) -> bool {
    eval(tab, "window.__marker === 'alive'") == serde_json::Value::Bool(true)
}

fn path_of(tab: &Tab) -> String {
    eval(tab, "location.pathname + location.search")
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn fill_login(tab: &Tab, tenant: &str, email: &str, password: &str) {
    type_into(tab, "input[name=tenant]", tenant);
    type_into(tab, "input[name=email]", email);
    type_into(tab, "input[name=password]", password);
}

#[tokio::test]
async fn recruiter_signs_in_navigates_and_signs_out() {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER_EMAIL, OWNER_PASSWORD).await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();

        // 1. защищённый маршрут без сессии ведёт на вход и помнит, куда шли
        tab.navigate_to(&format!("{base}/candidates")).unwrap();
        wait_until(
            &tab,
            "login form",
            "!!document.querySelector('input[name=password]')",
        );
        assert_eq!(path_of(&tab), "/login?next=/candidates");
        // дожидаемся гидратации: после неё форма отправляется без перезагрузки
        wait_until(&tab, "hydration", "document.querySelector('form') !== null");
        std::thread::sleep(Duration::from_millis(1500));
        set_marker(&tab);

        // 2. неверный пароль: ошибка на месте, страница не перезагружается
        fill_login(&tab, "acme", OWNER_EMAIL, "wrong-password");
        tab.find_element("button[type=submit]")
            .unwrap()
            .click()
            .unwrap();
        wait_until(
            &tab,
            "error notice",
            "!!document.querySelector('.notice--error')",
        );
        assert!(text_of(&tab, ".notice--error").contains("Неверная"));
        assert!(marker_alive(&tab), "login error must not reload the page");

        // 3. верные данные: попадаем на исходный маршрут, в шапке организация и пользователь
        tab.find_element("input[name=password]")
            .unwrap()
            .click()
            .unwrap();
        eval(
            &tab,
            "document.querySelector('input[name=password]').value = ''",
        );
        type_into(&tab, "input[name=password]", OWNER_PASSWORD);
        tab.find_element("button[type=submit]")
            .unwrap()
            .click()
            .unwrap();
        wait_until(
            &tab,
            "app shell",
            "!!document.querySelector('[data-testid=user-label]')",
        );
        assert_eq!(
            path_of(&tab),
            "/candidates",
            "returns to the requested route"
        );
        assert_eq!(text_of(&tab, "[data-testid=context-label]"), "acme");
        assert_eq!(text_of(&tab, "[data-testid=user-label]"), OWNER_EMAIL);
        assert!(text_of(&tab, "main").contains("Кандидатов пока нет"));
        assert!(marker_alive(&tab), "sign-in must not reload the page");

        // 4. навигация по разделам, разделы без домена показывают пустое состояние
        tab.find_element("nav a[href='/vacancies']")
            .unwrap()
            .click()
            .unwrap();
        wait_until(
            &tab,
            "vacancies",
            "document.querySelector('main').innerText.includes('Вакансий пока нет')",
        );
        tab.find_element("nav a[href='/interviews']")
            .unwrap()
            .click()
            .unwrap();
        wait_until(
            &tab,
            "interviews",
            "document.querySelector('main').innerText.includes('Интервью не запланированы')",
        );
        assert!(marker_alive(&tab), "navigation is client-side");

        // 5. обновление страницы сохраняет сессию (cookie), а не отправляет на вход
        tab.navigate_to(&format!("{base}/vacancies")).unwrap();
        wait_until(
            &tab,
            "reloaded shell",
            "!!document.querySelector('[data-testid=user-label]')",
        );
        assert_eq!(path_of(&tab), "/vacancies");

        // 6. выход возвращает на экран входа, защищённые маршруты снова закрыты
        set_marker(&tab);
        tab.find_element("header button[type=submit]")
            .unwrap()
            .click()
            .unwrap();
        wait_until(&tab, "login after logout", "location.pathname === '/login'");
        assert!(marker_alive(&tab), "sign-out must not reload the page");
        tab.navigate_to(&format!("{base}/candidates")).unwrap();
        wait_until(&tab, "login again", "location.pathname === '/login'");
    })
    .await
    .unwrap();

    stack.stop().await;
}
