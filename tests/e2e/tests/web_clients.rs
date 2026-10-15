//! Раздел «Заказчики» и привязка вакансий к заказчику в браузере.
mod common;

use std::time::Duration;

use common::*;
use e2e::Stack;
use serde_json::json;

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-password-1");

fn text(tab: &Tab, selector: &str) -> String {
    eval_str(
        tab,
        &format!("(document.querySelector({selector:?})||{{}}).innerText||''"),
    )
}

/// Выбирает в `select` вариант, текст которого содержит `label`, и сообщает фреймворку.
fn choose(tab: &Tab, selector: &str, label: &str) {
    wait_until(
        tab,
        &format!("option {label} in {selector}"),
        &format!(
            "Array.from(document.querySelectorAll({selector:?} + ' option')).some(o => o.textContent.includes({label:?}))"
        ),
    );
    eval(
        tab,
        &format!(
            "(() => {{ const s = document.querySelector({selector:?}); \
             const o = Array.from(s.options).find(o => o.textContent.includes({label:?})); \
             s.value = o.value; s.dispatchEvent(new Event('change', {{bubbles: true}})); }})()"
        ),
    );
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
async fn a_client_with_contacts_and_positions_end_to_end() {
    let stack = world().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);
        set_marker(&tab);

        // справочник должностей
        click(&tab, "nav a[href='/clients']");
        wait_until(&tab, "clients page", &exists_js("[data-testid=new-client]"));
        click(&tab, "[data-testid=positions-link]");
        wait_until(&tab, "positions page", &exists_js("[data-testid=position-input]"));
        for name in ["HR-директор", "CTO"] {
            type_into(&tab, "[data-testid=position-input]", name);
            click(&tab, "[data-testid=position-add]");
            wait_until(
                &tab,
                "position added",
                &format!("Array.from(document.querySelectorAll('[data-testid=position-row] input')).some(i => i.value === {name:?})"),
            );
        }
        assert_eq!(eval(&tab, &count_js("[data-testid=position-row]")), 2);

        // повтор названия: понятная ошибка
        type_into(&tab, "[data-testid=position-input]", "hr-директор");
        click(&tab, "[data-testid=position-add]");
        wait_until(&tab, "dup error", &exists_js("[data-testid=positions-error]"));
        assert!(text(&tab, "[data-testid=positions-error]").contains("уже есть"));

        // заказчик
        click(&tab, "nav a[href='/clients']");
        wait_until(&tab, "clients page", &exists_js("[data-testid=new-client]"));
        click(&tab, "[data-testid=new-client]");
        type_into(&tab, "[data-testid=client-name]", "Acme Corp");
        click(&tab, "[data-testid=client-submit]");
        wait_until(&tab, "card", "location.pathname.startsWith('/clients/')");
        wait_until(&tab, "title", "document.querySelector('[data-testid=client-title]') !== null");
        assert_eq!(text(&tab, "[data-testid=client-title]").trim(), "Acme Corp");
        assert!(marker_alive(&tab), "no reload");

        // два контакта с должностями
        for (name, position, email) in [
            ("Мария Петрова", "HR-директор", "maria@acme.io"),
            ("Иван Орлов", "CTO", "ivan@acme.io"),
        ] {
            click(&tab, "[data-testid=add-contact]");
            wait_until(&tab, "form", &exists_js("[data-testid=contact-form]"));
            type_into(&tab, "[data-testid=contact-fullname]", name);
            choose(&tab, "[data-testid=contact-position-select]", position);
            type_into(&tab, "[data-testid=contact-email]", email);
            click(&tab, "[data-testid=contact-submit]");
            wait_until(
                &tab,
                "contact listed",
                &format!("Array.from(document.querySelectorAll('[data-testid=contact-row]')).some(r => r.innerText.includes({name:?}) && r.innerText.includes({position:?}))"),
            );
        }
        assert_eq!(eval(&tab, &count_js("[data-testid=contact-row]")), 2);

        // некорректный email: ошибка у формы
        click(&tab, "[data-testid=add-contact]");
        wait_until(&tab, "form", &exists_js("[data-testid=contact-form]"));
        type_into(&tab, "[data-testid=contact-fullname]", "Без почты");
        type_into(&tab, "[data-testid=contact-email]", "не почта");
        click(&tab, "[data-testid=contact-submit]");
        wait_until(&tab, "email error", &exists_js("[data-testid=contact-error]"));
        click(&tab, "[data-testid=contact-cancel]");
        wait_until(&tab, "form closed", &gone_js("[data-testid=contact-form]"));

        // правка и удаление
        click(&tab, "[data-testid=edit-contact]");
        wait_until(&tab, "edit form", &exists_js("[data-testid=contact-form]"));
        replace_text(&tab, "[data-testid=contact-fullname]", "Мария Петрова-Иванова");
        click(&tab, "[data-testid=contact-submit]");
        wait_until(
            &tab,
            "renamed",
            "document.querySelector('[data-testid=contacts]').innerText.includes('Петрова-Иванова')",
        );
        click(&tab, "[data-testid=delete-contact]");
        wait_count(&tab, "one contact left", "[data-testid=contact-row]", 1);

        // переименование должности видно у контакта
        click(&tab, "nav a[href='/clients']");
        wait_until(&tab, "clients page", &exists_js("[data-testid=positions-link]"));
        click(&tab, "[data-testid=positions-link]");
        wait_until(&tab, "positions", &exists_js("[data-testid=position-row]"));
        // по алфавиту «CTO» первая (её держит Иван Орлов); переименовываем её
        replace_text_at(&tab, 0, "Технический директор");
        click_nth(&tab, "[data-testid=position-rename]", 0);
        wait_until(
            &tab,
            "renamed position",
            "Array.from(document.querySelectorAll('[data-testid=position-name-input]')).some(i => i.value === 'Технический директор')",
        );

        // после переименования порядок: «HR-директор», «Технический директор» (используется)
        wait_until(
            &tab,
            "order after rename",
            "document.querySelectorAll('[data-testid=position-name-input]')[1].value === 'Технический директор'",
        );
        click_nth(&tab, "[data-testid=position-delete]", 1);
        wait_until(&tab, "in use", &exists_js("[data-testid=positions-error]"));
        assert!(text(&tab, "[data-testid=positions-error]").contains("назначена"));
    })
    .await
    .unwrap();
    stack.stop().await;
}

fn replace_text_at(tab: &Tab, index: usize, value: &str) {
    eval(
        tab,
        &format!(
            "(() => {{ const el = document.querySelectorAll('[data-testid=position-name-input]')[{index}]; \
             el.focus(); el.value = {value:?}; el.dispatchEvent(new Event('input', {{bubbles: true}})); }})()"
        ),
    );
}

fn click_nth(tab: &Tab, selector: &str, index: usize) {
    eval(
        tab,
        &format!("document.querySelectorAll({selector:?})[{index}].click()"),
    );
}

#[tokio::test]
async fn a_duplicate_name_and_archiving() {
    let stack = world().await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    let acme = stack.add_client(&owner, "Acme Corp").await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);

        click(&tab, "nav a[href='/clients']");
        wait_until(&tab, "list", &exists_js("[data-testid=client-row]"));
        click(&tab, "[data-testid=new-client]");
        type_into(&tab, "[data-testid=client-name]", "  acme   CORP ");
        click(&tab, "[data-testid=client-submit]");
        wait_until(&tab, "dup", "document.body.innerText.includes('уже есть')");
        assert_eq!(path_of(&tab), "/clients");

        // архивация скрывает заказчика из списка, «с архивными» возвращает
        tab.navigate_to(&format!("{base}/clients/{acme}")).unwrap();
        wait_until(&tab, "card", &exists_js("[data-testid=archive-toggle]"));
        std::thread::sleep(Duration::from_millis(1200));
        click(&tab, "[data-testid=archive-toggle]");
        wait_until(&tab, "archived", &exists_js("[data-testid=archived-chip]"));
        click(&tab, "nav a[href='/clients']");
        wait_until(&tab, "empty list", &exists_js(".empty"));
        click(&tab, "[data-testid=show-archived]");
        wait_until(
            &tab,
            "with archived",
            &exists_js("[data-testid=client-row]"),
        );
        assert!(text(&tab, "[data-testid=client-row]").contains("архив"));
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn a_vacancy_is_created_for_a_client_and_shows_it() {
    let stack = world().await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    let acme = stack.add_client(&owner, "Acme Corp").await;
    let globex = stack.add_client(&owner, "Globex").await;
    let position = owner
        .post("/api/positions", json!({"name": "HR-директор"}))
        .await;
    let position: serde_json::Value = position.json().await.unwrap();
    for (client, name) in [(&acme, "Мария Петрова"), (&globex, "Глеб Орлов")] {
        let res = owner
            .post(
                &format!("/api/clients/{client}/contacts"),
                json!({"full_name": name, "position_id": position["id"]}),
            )
            .await;
        assert_eq!(res.status(), 201);
    }
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);

        tab.navigate_to(&format!("{base}/vacancies?new=1")).unwrap();
        wait_until(&tab, "form", &exists_js("[data-testid=vacancy-form]"));
        std::thread::sleep(Duration::from_millis(1200));

        // без заказчика вакансию создать нельзя
        type_into(&tab, "[data-testid=vf-title]", "Senior Go");
        click(&tab, "[data-testid=vf-submit]");
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(eval(&tab, &count_js("[data-testid=vacancy-facts]")), 0);

        // контакты зависят от выбранного заказчика
        choose(&tab, "[data-testid=vf-client]", "Acme Corp");
        wait_until(
            &tab,
            "acme contacts",
            "Array.from(document.querySelectorAll('[data-testid=vf-contact] option')).some(o => o.textContent.includes('Мария Петрова — HR-директор'))",
        );
        choose(&tab, "[data-testid=vf-client]", "Globex");
        wait_until(
            &tab,
            "globex contacts",
            "Array.from(document.querySelectorAll('[data-testid=vf-contact] option')).some(o => o.textContent.includes('Глеб Орлов')) \
             && !Array.from(document.querySelectorAll('[data-testid=vf-contact] option')).some(o => o.textContent.includes('Мария'))",
        );
        choose(&tab, "[data-testid=vf-client]", "Acme Corp");
        choose(&tab, "[data-testid=vf-contact]", "Мария Петрова");
        click(&tab, "[data-testid=vf-submit]");

        wait_until(&tab, "facts", &exists_js("[data-testid=vacancy-client]"));
        let line = text(&tab, "[data-testid=vacancy-client]");
        assert!(
            line.contains("Acme Corp") && line.contains("Мария Петрова, HR-директор"),
            "{line}"
        );
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn a_viewer_reads_clients_but_cannot_change_them() {
    let stack = world().await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    let acme = stack.add_client(&owner, "Acme Corp").await;
    let res = owner
        .post(
            &format!("/api/clients/{acme}/contacts"),
            json!({"full_name": "Мария Петрова"}),
        )
        .await;
    assert_eq!(res.status(), 201);
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, VIEWER);

        click(&tab, "nav a[href='/clients']");
        wait_until(&tab, "list", &exists_js("[data-testid=client-row]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=new-client]")), 0);

        tab.navigate_to(&format!("{base}/clients/{acme}")).unwrap();
        wait_until(&tab, "card", &exists_js("[data-testid=contact-row]"));
        assert!(text(&tab, "[data-testid=contacts]").contains("Мария Петрова"));
        for hidden in [
            "add-contact",
            "edit-contact",
            "delete-contact",
            "archive-toggle",
            "client-edit",
        ] {
            assert_eq!(
                eval(&tab, &count_js(&format!("[data-testid={hidden}]"))),
                0,
                "{hidden}"
            );
        }
    })
    .await
    .unwrap();
    stack.stop().await;
}
