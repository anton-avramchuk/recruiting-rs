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
        wait_until(&tab, "list", &exists_js("[data-testid=grid-row]"));
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
        choose(&tab, "[data-testid=grid-filter-archived]", "С архивными");
        wait_until(&tab, "with archived", &exists_js("[data-testid=grid-row]"));
        assert!(text(&tab, "[data-testid=grid-row]").contains("архив"));
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
        wait_until(&tab, "list", &exists_js("[data-testid=grid-row]"));
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

#[tokio::test]
async fn applications_can_be_filtered_by_client_from_the_card_and_the_screen() {
    let stack = world().await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    let acme = stack.add_client(&owner, "Acme Corp").await;
    let globex = stack.add_client(&owner, "Globex").await;
    for (client, title, people) in [
        (&acme, "Senior Go", ["Анна Смирнова", "Борис Иванов"]),
        (&globex, "QA инженер", ["Вера Орлова", "Глеб Петров"]),
    ] {
        let vacancy: serde_json::Value = owner
            .post(
                "/api/vacancies",
                json!({"title": title, "client_id": client}),
            )
            .await
            .json()
            .await
            .unwrap();
        for name in people {
            let person: serde_json::Value = owner
                .post("/api/persons", json!({"full_name": name}))
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
    }
    let base = stack.web_crm_url.clone();
    let acme_id = acme.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);

        // с карточки заказчика - к его откликам
        tab.navigate_to(&format!("{base}/clients/{acme_id}")).unwrap();
        wait_until(&tab, "card", &exists_js("[data-testid=client-applications]"));
        std::thread::sleep(Duration::from_millis(1200));
        click(&tab, "[data-testid=client-applications]");
        wait_until(&tab, "applications", "location.pathname === '/applications'");
        wait_count(&tab, "two rows of the client", "[data-testid=app-row]", 2);
        assert_eq!(
            eval_str(&tab, "document.querySelector('[data-testid=filter-client]').value"),
            acme_id
        );
        let rows = text(&tab, "[data-testid=applications]");
        assert!(rows.contains("Анна Смирнова") && !rows.contains("Вера Орлова"), "{rows}");
        set_marker(&tab);

        // другой заказчик на экране
        choose(&tab, "[data-testid=filter-client]", "Globex");
        wait_until(
            &tab,
            "globex rows",
            "document.querySelector('[data-testid=applications]').innerText.includes('Вера Орлова')              && !document.querySelector('[data-testid=applications]').innerText.includes('Анна Смирнова')",
        );
        assert!(marker_alive(&tab), "no reload");

        // поиск по названию заказчика находит отклики на его вакансии
        choose(&tab, "[data-testid=filter-client]", "Все заказчики");
        wait_count(&tab, "all rows", "[data-testid=app-row]", 4);
        type_into(&tab, "[data-testid=search]", "acme");
        wait_count(&tab, "found by client name", "[data-testid=app-row]", 2);

        // сброс возвращает всё
        click(&tab, "[data-testid=reset-filters]");
        wait_count(&tab, "after reset", "[data-testid=app-row]", 4);
        assert_eq!(
            eval_str(&tab, "document.querySelector('[data-testid=filter-client]').value"),
            ""
        );
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn candidates_and_vacancies_can_be_filtered_by_client() {
    let stack = world().await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    let acme = stack.add_client(&owner, "Acme Corp").await;
    let globex = stack.add_client(&owner, "Globex").await;
    stack.add_client(&owner, "Пустой заказчик").await;
    let mut vacancy_ids = Vec::new();
    for (client, title) in [
        (&acme, "Senior Go"),
        (&acme, "Rust-разработчик"),
        (&globex, "QA инженер"),
    ] {
        let v: serde_json::Value = owner
            .post(
                "/api/vacancies",
                json!({"title": title, "client_id": client}),
            )
            .await
            .json()
            .await
            .unwrap();
        vacancy_ids.push(v["id"].clone());
    }
    let mut people = Vec::new();
    for name in [
        "Анна Смирнова",
        "Борис Иванов",
        "Вера Орлова",
        "Одинокий Кандидат",
    ] {
        let person: serde_json::Value = owner
            .post("/api/persons", json!({"full_name": name}))
            .await
            .json()
            .await
            .unwrap();
        people.push(person["id"].clone());
    }
    // Анна и Борис - у Acme (Анна дважды), Вера - у Globex, Одинокий - без откликов
    for (person, vacancy) in [(0, 0), (0, 1), (1, 0), (2, 2)] {
        let res = owner
            .post(
                "/api/applications",
                json!({"person_id": people[person], "vacancy_id": vacancy_ids[vacancy]}),
            )
            .await;
        assert_eq!(res.status(), 201);
    }
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);

        // кандидаты заказчика
        click(&tab, "nav a[href='/candidates']");
        wait_count(&tab, "all people", "[data-testid=persons] tbody tr", 4);
        std::thread::sleep(Duration::from_millis(1200));
        set_marker(&tab);
        choose(&tab, "[data-testid=filter-client]", "Acme Corp");
        wait_count(&tab, "people of acme", "[data-testid=persons] tbody tr", 2);
        let rows = text(&tab, "[data-testid=persons]");
        assert!(
            rows.contains("Анна Смирнова") && rows.contains("Борис Иванов"),
            "{rows}"
        );
        assert!(
            !rows.contains("Вера Орлова") && !rows.contains("Одинокий"),
            "{rows}"
        );
        assert!(
            text(&tab, "main").contains("Найдено: 2"),
            "different people, not applications"
        );

        // вместе с поиском
        type_into(&tab, "[data-testid=search]", "борис");
        wait_count(
            &tab,
            "search within the client",
            "[data-testid=persons] tbody tr",
            1,
        );

        // сброс возвращает всех
        click(&tab, "[data-testid=reset-filters]");
        wait_count(&tab, "everyone again", "[data-testid=persons] tbody tr", 4);
        assert_eq!(
            eval_str(
                &tab,
                "document.querySelector('[data-testid=filter-client]').value"
            ),
            ""
        );

        // заказчик, на чьи вакансии никто не откликался
        choose(&tab, "[data-testid=filter-client]", "Пустой заказчик");
        wait_until(
            &tab,
            "empty note",
            "document.querySelector('main').innerText.includes('Кандидатов заказчика пока нет')",
        );
        assert!(marker_alive(&tab), "no reload");

        // вакансии заказчика
        click(&tab, "nav a[href='/vacancies']");
        wait_until(&tab, "board", &exists_js("[data-testid=vacancy-select]"));
        std::thread::sleep(Duration::from_millis(1200));
        assert_eq!(
            eval(&tab, &count_js("[data-testid=vacancy-select] option")),
            3
        );
        choose(&tab, "[data-testid=vacancy-client-filter]", "Globex");
        wait_until(
            &tab,
            "one vacancy",
            "document.querySelectorAll('[data-testid=vacancy-select] option').length === 1",
        );
        assert!(text(&tab, "[data-testid=vacancy-select]").contains("QA инженер"));
        choose(&tab, "[data-testid=vacancy-client-filter]", "Acme Corp");
        wait_until(
            &tab,
            "two vacancies",
            "document.querySelectorAll('[data-testid=vacancy-select] option').length === 2",
        );
        // у заказчика без вакансий - пустое состояние
        choose(
            &tab,
            "[data-testid=vacancy-client-filter]",
            "Пустой заказчик",
        );
        wait_until(
            &tab,
            "empty vacancies",
            "document.querySelector('main').innerText.includes('У заказчика пока нет вакансий')",
        );
        choose(&tab, "[data-testid=vacancy-client-filter]", "Все заказчики");
        wait_until(
            &tab,
            "all vacancies",
            "document.querySelectorAll('[data-testid=vacancy-select] option').length === 3",
        );
    })
    .await
    .unwrap();
    stack.stop().await;
}
