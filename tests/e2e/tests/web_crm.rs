//! Клиентское приложение в настоящем браузере (headless Chrome): вход, защита маршрутов, каркас.
mod common;

use std::time::Duration;

use common::*;
use e2e::Stack;

const OWNER_EMAIL: &str = "boss@acme.io";
const OWNER_PASSWORD: &str = "boss-password-1";

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
        tab.find_element("nav a[href='/applications']")
            .unwrap()
            .click()
            .unwrap();
        wait_until(
            &tab,
            "applications",
            "document.querySelector('main').innerText.includes('Откликов пока нет')",
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
