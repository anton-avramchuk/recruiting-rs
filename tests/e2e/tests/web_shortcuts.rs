//! Клавиши списка «Отклики» и панели человека в браузере.
mod common;

use std::time::Duration;

use common::*;
use e2e::Stack;
use serde_json::{Value, json};

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-password-1");
const NAMES: [&str; 3] = ["Анна Смирнова", "Борис Иванов", "Вера Орлова"];

fn text(tab: &Tab, selector: &str) -> String {
    eval_str(
        tab,
        &format!("(document.querySelector({selector:?})||{{}}).innerText||''"),
    )
}

async fn seed() -> Stack {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    stack.add_user(&owner, VIEWER.0, VIEWER.1, "viewer").await;
    let vacancy: Value = owner
        .post(
            "/api/vacancies",
            json!({"title": "Senior Go", "recruiter": "Мария"}),
        )
        .await
        .json()
        .await
        .unwrap();
    for name in NAMES {
        let person: Value = owner
            .post(
                "/api/persons",
                json!({"full_name": name, "source": "hh.ru"}),
            )
            .await
            .json()
            .await
            .unwrap();
        let res = owner
            .post(
                "/api/applications",
                json!({"person_id": person["id"], "vacancy_id": vacancy["id"]}),
            )
            .await;
        assert_eq!(res.status(), 201);
    }
    stack
}

fn open_list(tab: &Tab, base: &str, user: (&str, &str)) {
    login_crm(tab, base, "acme", user.0, user.1);
    std::thread::sleep(Duration::from_millis(1500)); // гидратация
    click(tab, "nav a[href='/applications']");
    wait_count(tab, "rows", "[data-testid=app-row]", 3);
    std::thread::sleep(Duration::from_millis(800));
}

fn cursor(tab: &Tab) -> i64 {
    eval(
        tab,
        "Array.from(document.querySelectorAll('[data-testid=app-row]')).findIndex(r => r.classList.contains('row-cursor'))",
    )
    .as_i64()
    .unwrap_or(-2)
}

fn selected(tab: &Tab) -> i64 {
    eval(tab, &count_js("tr.row-selected"))
        .as_i64()
        .unwrap_or(-1)
}

fn row_stage(tab: &Tab, index: usize) -> String {
    eval_str(
        tab,
        &format!(
            "document.querySelectorAll('[data-testid=app-row] [data-testid=row-stage]')[{index}].innerText"
        ),
    )
}

fn row_name(tab: &Tab, index: usize) -> String {
    eval_str(
        tab,
        &format!(
            "document.querySelectorAll('[data-testid=app-row] [data-testid=open-app]')[{index}].innerText"
        ),
    )
}

#[tokio::test]
async fn the_cursor_walks_the_list_and_opens_the_panel() {
    let stack = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        open_list(&tab, &base, OWNER);
        assert_eq!(cursor(&tab), -1, "no cursor until a key is pressed");

        tab.press_key("j").unwrap();
        wait_until(&tab, "first row", "document.querySelectorAll('tr.row-cursor').length === 1");
        assert_eq!(cursor(&tab), 0);
        tab.press_key("j").unwrap();
        tab.press_key("j").unwrap();
        tab.press_key("j").unwrap();
        wait_until(
            &tab,
            "last row",
            "Array.from(document.querySelectorAll('[data-testid=app-row]')).findIndex(r => r.classList.contains('row-cursor')) === 2",
        );
        tab.press_key("k").unwrap();
        wait_until(
            &tab,
            "second row",
            "Array.from(document.querySelectorAll('[data-testid=app-row]')).findIndex(r => r.classList.contains('row-cursor')) === 1",
        );

        let name = row_name(&tab, 1);
        tab.press_key("Enter").unwrap();
        wait_until(&tab, "panel", "!!document.querySelector('[data-testid=panel-name]')");
        assert_eq!(text(&tab, "[data-testid=panel-name]").trim(), name.trim());

        // в панели J не двигает курсор списка
        tab.press_key("j").unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(cursor(&tab), 1);

        // Esc закрывает панель
        tab.press_key("Escape").unwrap();
        wait_until(&tab, "panel closed", &gone_js("[data-testid=panel]"));

        // смена фильтра возвращает курсор в начало
        type_into(&tab, "[data-testid=filter-source]", "hh");
        wait_until(&tab, "reset", "document.querySelectorAll('tr.row-cursor').length === 0");
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn selection_and_group_keys_work_without_the_mouse() {
    let stack = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        open_list(&tab, &base, OWNER);

        // без выделения E ничего не делает
        tab.press_key("e").unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(eval(&tab, &count_js("[data-testid=bulk-result]")), 0);

        // X выделяет строку под курсором и снимает при повторе
        tab.press_key("x").unwrap(); // курсора ещё нет
        assert_eq!(selected(&tab), 0);
        tab.press_key("j").unwrap();
        tab.press_key("x").unwrap();
        wait_until(
            &tab,
            "one selected",
            "document.querySelectorAll('tr.row-selected').length === 1",
        );
        assert_eq!(
            text(&tab, "[data-testid=selected-count]").trim(),
            "Выбрано: 1"
        );
        tab.press_key("x").unwrap();
        wait_until(
            &tab,
            "none selected",
            "document.querySelectorAll('tr.row-selected').length === 0",
        );

        // A выделяет всё, Esc снимает
        tab.press_key("a").unwrap();
        wait_until(
            &tab,
            "all selected",
            "document.querySelectorAll('tr.row-selected').length === 3",
        );
        assert!(text(&tab, "[data-testid=bulkbar]").contains("E"));
        tab.press_key("Escape").unwrap();
        wait_until(
            &tab,
            "cleared",
            "document.querySelectorAll('tr.row-selected').length === 0",
        );

        // E переводит выделенные дальше
        let before = row_stage(&tab, 0);
        tab.press_key("j").unwrap();
        tab.press_key("x").unwrap();
        tab.press_key("j").unwrap();
        tab.press_key("x").unwrap();
        wait_until(
            &tab,
            "two selected",
            "document.querySelectorAll('tr.row-selected').length === 2",
        );
        tab.press_key("e").unwrap();
        wait_until(&tab, "bulk result", &exists_js("[data-testid=bulk-result]"));
        assert!(text(&tab, "[data-testid=bulk-result]").contains("Изменено: 2"));
        wait_until(
            &tab,
            "two applications moved on",
            "Array.from(document.querySelectorAll('[data-testid=row-stage]')).filter(e => e.innerText !== 'Новый').length === 2",
        );
        assert_eq!(before, "Новый");
        wait_until(
            &tab,
            "selection cleared",
            "document.querySelectorAll('tr.row-selected').length === 0",
        );
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn digits_set_the_stage_in_the_list_and_in_the_panel() {
    let stack = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        open_list(&tab, &base, OWNER);

        // цифра текущего этапа запроса не шлёт: все отклики новые, это этап 1
        tab.press_key("j").unwrap();
        wait_until(&tab, "cursor", "document.querySelectorAll('tr.row-cursor').length === 1");
        eval(&tab, "performance.clearResourceTimings()");
        tab.press_key("1").unwrap();
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(
            eval(&tab, "performance.getEntriesByType('resource').filter(e => e.name.includes('/stage')).length"),
            0
        );

        // цифра 3 переводит отклик под курсором на «Интервью HR»
        tab.press_key("3").unwrap();
        wait_until(
            &tab,
            "stage 3 in the list",
            "Array.from(document.querySelectorAll('[data-testid=row-stage]')).filter(e => e.innerText === 'Интервью HR').length === 1",
        );

        // в панели цифра меняет этап открытого отклика, E двигает дальше
        tab.press_key("Enter").unwrap();
        wait_until(&tab, "panel", "!!document.querySelector('[data-testid=panel-name]')");
        tab.press_key("6").unwrap();
        wait_until(
            &tab,
            "stage 6 in the panel",
            "document.querySelector('[data-testid=panel-stage]') && document.querySelector('[data-testid=panel-stage]').innerText === 'Оффер'",
        );
        tab.press_key("e").unwrap();
        wait_until(
            &tab,
            "advanced",
            "document.querySelector('[data-testid=panel-stage]') && document.querySelector('[data-testid=panel-stage]').innerText === 'Выход'",
        );
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn panel_keys_open_forms_and_escape_peels_one_layer_at_a_time() {
    let stack = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        open_list(&tab, &base, OWNER);
        tab.press_key("j").unwrap();
        tab.press_key("a").unwrap(); // выделение всех: понадобится для проверки Esc
        wait_until(
            &tab,
            "selected",
            "document.querySelectorAll('tr.row-selected').length === 3",
        );
        tab.press_key("Enter").unwrap();
        wait_until(
            &tab,
            "panel",
            "!!document.querySelector('[data-testid=panel-name]')",
        );

        // R открывает форму отказа и ставит фокус в причину
        tab.press_key("r").unwrap();
        wait_until(
            &tab,
            "reject form",
            &exists_js("[data-testid=reject-reason]"),
        );
        wait_until(
            &tab,
            "reason focused",
            "document.activeElement && document.activeElement.dataset.testid === 'reject-reason'",
        );
        // Esc из поля снимает фокус, второй — закрывает форму, третий — панель, четвёртый — выделение
        tab.press_key("Escape").unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(eval(&tab, &count_js("[data-testid=reject-reason]")), 1);
        tab.press_key("Escape").unwrap();
        wait_until(&tab, "form closed", &gone_js("[data-testid=reject-reason]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=panel]")), 1);

        // S открывает форму интервью
        tab.press_key("s").unwrap();
        wait_until(
            &tab,
            "interview form",
            &exists_js("[data-testid=interview-at]"),
        );
        tab.press_key("Escape").unwrap();
        tab.press_key("Escape").unwrap();
        wait_until(&tab, "form closed", &gone_js("[data-testid=interview-at]"));

        // N — в заметку; набранные буквы попадают в неё и не запускают действия
        tab.press_key("n").unwrap();
        wait_until(
            &tab,
            "note focused",
            "document.activeElement && document.activeElement.dataset.testid === 'note'",
        );
        tab.type_str("re").unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            eval_str(&tab, "document.querySelector('[data-testid=note]').value"),
            "re"
        );
        assert_eq!(eval(&tab, &count_js("[data-testid=reject-reason]")), 0);
        tab.press_key("Escape").unwrap(); // снять фокус
        tab.press_key("Escape").unwrap(); // закрыть панель
        wait_until(&tab, "panel closed", &gone_js("[data-testid=panel]"));
        assert_eq!(
            selected(&tab),
            3,
            "selection survives while the panel closes"
        );
        tab.press_key("Escape").unwrap();
        wait_until(
            &tab,
            "selection cleared",
            "document.querySelectorAll('tr.row-selected').length === 0",
        );
        assert!(row_name(&tab, 0).len() > 2);
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn a_viewer_can_move_and_open_but_not_change_anything() {
    let stack = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        open_list(&tab, &base, VIEWER);
        let before = row_stage(&tab, 0);

        tab.press_key("j").unwrap();
        wait_until(
            &tab,
            "cursor",
            "document.querySelectorAll('tr.row-cursor').length === 1",
        );
        for key in ["x", "a", "e", "r", "m", "3"] {
            tab.press_key(key).unwrap();
        }
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(eval(&tab, &count_js("tr.row-selected")), 0);
        assert_eq!(row_stage(&tab, 0), before);

        tab.press_key("Enter").unwrap();
        wait_until(
            &tab,
            "panel",
            "!!document.querySelector('[data-testid=panel-name]')",
        );
        for key in ["r", "s", "5", "e"] {
            tab.press_key(key).unwrap();
        }
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(eval(&tab, &count_js("[data-testid=reject-reason]")), 0);
        assert_eq!(eval(&tab, &count_js("[data-testid=interview-at]")), 0);
        tab.press_key("Escape").unwrap();
        wait_until(&tab, "panel closed", &gone_js("[data-testid=panel]"));
        assert_eq!(row_stage(&tab, 0), before);
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn typing_in_a_field_and_open_overlays_do_not_trigger_list_keys() {
    let stack = seed().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        open_list(&tab, &base, OWNER);

        type_into(&tab, "[data-testid=search]", "jax");
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(cursor(&tab), -1);
        assert_eq!(selected(&tab), 0);
        // Esc из поля снимает фокус, после этого J снова двигает курсор
        eval(&tab, "document.querySelector('[data-testid=search]').value = ''; document.querySelector('[data-testid=search]').dispatchEvent(new Event('input', {bubbles: true}))");
        wait_count(&tab, "rows", "[data-testid=app-row]", 3);
        tab.press_key("Escape").unwrap();
        wait_until(
            &tab,
            "blurred",
            "!document.activeElement || document.activeElement.tagName === 'BODY'",
        );
        tab.press_key("j").unwrap();
        wait_until(&tab, "cursor", "document.querySelectorAll('tr.row-cursor').length === 1");

        // при открытой шпаргалке J молчит, а Esc закрывает только её
        tab.press_key("?").unwrap();
        wait_until(&tab, "cheatsheet", &exists_js("[data-testid=cheatsheet]"));
        tab.press_key("j").unwrap();
        tab.press_key("x").unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(cursor(&tab), 0);
        assert_eq!(selected(&tab), 0);
        tab.press_key("Escape").unwrap();
        wait_until(&tab, "closed", &gone_js("[data-testid=cheatsheet]"));

        // шпаргалка перечисляет клавиши списка
        tab.press_key("?").unwrap();
        wait_until(&tab, "cheatsheet", &exists_js("[data-testid=cheatsheet]"));
        let sheet = text(&tab, "[data-testid=cheatsheet]");
        for needle in ["Enter", "Выделить строку", "Перевести на этап", "Написать выделенным", "Назначить интервью"] {
            assert!(sheet.contains(needle), "{needle}: {sheet}");
        }
    })
    .await
    .unwrap();
    stack.stop().await;
}
