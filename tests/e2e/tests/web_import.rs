//! Импорт кандидатов в браузере: файл разбирается на месте, на сервер уходят только строки.
mod common;

use std::{path::PathBuf, time::Duration};

use common::*;
use e2e::Stack;
use serde_json::{Value, json};

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-password-1");

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/candidate-import/tests/fixtures")
        .join(name);
    let path = std::fs::canonicalize(path).unwrap();
    path.display()
        .to_string()
        .trim_start_matches(r"\\?\")
        .to_string()
}

fn temp_file(name: &str, content: &str) -> String {
    let dir = std::env::temp_dir().join(format!("import-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path.display().to_string()
}

fn text(tab: &Tab, selector: &str) -> String {
    eval_str(
        tab,
        &format!("(document.querySelector({selector:?})||{{}}).innerText||''"),
    )
}

/// Открывает мастер импорта и выбирает файл.
fn pick(tab: &Tab, base: &str, path: &str) {
    tab.navigate_to(&format!("{base}/candidates/import"))
        .unwrap();
    wait_until(tab, "wizard", &exists_js("[data-testid=file-input]"));
    std::thread::sleep(Duration::from_millis(1500)); // гидратация
    set_file(tab, path);
}

fn set_file(tab: &Tab, path: &str) {
    tab.find_element("[data-testid=file-input]")
        .unwrap()
        .set_input_files(&[path])
        .unwrap();
}

async fn setup() -> (Stack, Value) {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    stack.add_user(&owner, VIEWER.0, VIEWER.1, "viewer").await;
    let vacancy: Value = owner
        .post("/api/vacancies", json!({"title": "Go-разработчик"}))
        .await
        .json()
        .await
        .unwrap();
    (stack, vacancy)
}

fn choose_vacancy(tab: &Tab, id: &str) {
    wait_until(
        tab,
        "vacancy options",
        "document.querySelectorAll('[data-testid=import-vacancy] option').length === 2",
    );
    eval(
        tab,
        &format!(
            "(() => {{ const el = document.querySelector('[data-testid=import-vacancy]'); el.value = {id:?}; \
             el.dispatchEvent(new Event('change', {{bubbles: true}})); }})()"
        ),
    );
}

#[tokio::test]
async fn csv_is_imported_with_applications_and_a_second_import_adds_nothing() {
    let (stack, vacancy) = setup().await;
    let base = stack.web_crm_url.clone();
    let vacancy_id = vacancy["id"].as_str().unwrap().to_string();
    let csv = fixture("demo-kandidaty.csv");

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        tab.navigate_to(&format!("{base}/candidates")).unwrap();
        wait_until(&tab, "import button", &exists_js("[data-testid=import-button]"));
        click(&tab, "[data-testid=import-button]");
        wait_until(&tab, "wizard", &exists_js("[data-testid=file-input]"));
        std::thread::sleep(Duration::from_millis(1500));

        // запросы с телом записываем, чтобы убедиться: файл не уходит, уходят строки
        eval(
            &tab,
            "(() => { window.__bodies = []; const original = window.fetch.bind(window);              window.fetch = (input, init) => {                  if (input instanceof Request) { input.clone().text().then(t => window.__bodies.push(t.slice(0, 4000))); }                  else if (init && init.body) { window.__bodies.push(String(init.body).slice(0, 4000)); }                  return original(input, init); }; })()",
        );
        set_file(&tab, &csv);

        // сопоставление колонок уже сделано
        wait_until(&tab, "mapping", &exists_js("[data-testid=import-table]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=map-row]")), 9);
        let mapped = eval_str(
            &tab,
            "Array.from(document.querySelectorAll('[data-testid=map-row] select')).map(s => s.value).join(',')",
        );
        assert_eq!(mapped, "name,phone,email,city,role,salary,tags,jobs,edu");
        assert!(text(&tab, "[data-testid=import-hint]").contains("имя найдено"));
        choose_vacancy(&tab, &vacancy_id);
        click(&tab, "[data-testid=import-start]");

        wait_until(&tab, "done", &exists_js("[data-testid=import-done]"));
        assert_eq!(text(&tab, "[data-testid=sum-created]").trim(), "2");
        assert_eq!(text(&tab, "[data-testid=sum-merged]").trim(), "0");
        assert_eq!(text(&tab, "[data-testid=sum-rejected]").trim(), "0");
        assert_eq!(text(&tab, "[data-testid=sum-applications]").trim(), "2");

        let bodies = eval_str(&tab, "JSON.stringify(window.__bodies)");
        assert!(bodies.contains("\\\"rows\\\""), "rows were sent: {bodies}");
        assert!(!bodies.contains("PK"), "the file itself never goes out");
        assert!(!bodies.contains("demo-kandidaty.csv\\\\"), "no file paths either");

        // повторная загрузка того же файла: людей не прибавилось
        eval(&tab, "document.querySelectorAll('[data-testid=import-done] button')[0].click()");
        wait_until(&tab, "picker again", &exists_js("[data-testid=file-input]"));
        set_file(&tab, &csv);
        wait_until(&tab, "mapping again", &exists_js("[data-testid=import-table]"));
        choose_vacancy(&tab, &vacancy_id);
        click(&tab, "[data-testid=import-start]");
        wait_until(&tab, "done again", &exists_js("[data-testid=import-done]"));
        assert_eq!(text(&tab, "[data-testid=sum-created]").trim(), "0");
        assert_eq!(text(&tab, "[data-testid=sum-merged]").trim(), "2");
        assert_eq!(text(&tab, "[data-testid=sum-applications]").trim(), "0");
    })
    .await
    .unwrap();

    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    let people: Value = owner.get("/api/persons").await.json().await.unwrap();
    assert_eq!(people["total"], 2);
    let apps: Value = owner.get("/api/applications").await.json().await.unwrap();
    assert_eq!(apps["total"], 2);
    let id = apps["items"][0]["id"].as_str().unwrap();
    let detail: Value = owner
        .get(&format!("/api/applications/{id}"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        detail["history"][0]["text"],
        "Импортирован из файла demo-kandidaty.csv"
    );
    stack.stop().await;
}

#[tokio::test]
async fn xlsx_with_a_broken_checksum_is_imported() {
    let (stack, _) = setup().await;
    let base = stack.web_crm_url.clone();
    let file = fixture("demo-kandidaty.xlsx");

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        pick(&tab, &base, &file);
        wait_until(&tab, "mapping", &exists_js("[data-testid=import-table]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=map-row]")), 7);
        click(&tab, "[data-testid=import-start]");
        wait_until(&tab, "done", &exists_js("[data-testid=import-done]"));
        assert_eq!(text(&tab, "[data-testid=sum-created]").trim(), "1");
    })
    .await
    .unwrap();

    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    let people: Value = owner
        .get("/api/persons?q=%D0%98%D1%80%D0%B8%D0%BD%D0%B0")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(people["total"], 1);
    assert_eq!(people["items"][0]["full_name"], "Ирина Соколова");
    assert_eq!(people["items"][0]["city"], "Казань");
    assert_eq!(people["items"][0]["expected_salary"], 210000);
    stack.stop().await;
}

#[tokio::test]
async fn a_pdf_resume_is_checked_edited_and_imported() {
    let (stack, vacancy) = setup().await;
    let base = stack.web_crm_url.clone();
    let vacancy_id = vacancy["id"].as_str().unwrap().to_string();
    let file = fixture("demo-rezyume.pdf");

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        pick(&tab, &base, &file);
        wait_until(&tab, "resume form", &exists_js("[data-testid=resume-form]"));
        assert_eq!(
            eval_str(
                &tab,
                "document.querySelector('[data-testid=rf-name]').value"
            ),
            "Ирина Соколова"
        );
        assert_eq!(
            eval_str(
                &tab,
                "document.querySelector('[data-testid=rf-city]').value"
            ),
            "Казань"
        );
        // правим город до импорта
        set_value(&tab, "[data-testid=rf-city]", "Самара");
        choose_vacancy(&tab, &vacancy_id);
        click(&tab, "[data-testid=import-start]");
        wait_until(&tab, "done", &exists_js("[data-testid=import-done]"));
        assert_eq!(text(&tab, "[data-testid=sum-created]").trim(), "1");
        assert_eq!(text(&tab, "[data-testid=sum-applications]").trim(), "1");
    })
    .await
    .unwrap();

    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    let people: Value = owner.get("/api/persons").await.json().await.unwrap();
    let id = people["items"][0]["id"].as_str().unwrap().to_string();
    let card: Value = owner
        .get(&format!("/api/persons/{id}"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(card["full_name"], "Ирина Соколова");
    assert_eq!(card["city"], "Самара");
    assert_eq!(card["email"], "irina@mail.ru");
    assert_eq!(card["employments"].as_array().unwrap().len(), 3);
    assert_eq!(card["educations"].as_array().unwrap().len(), 1);
    assert_eq!(
        card["skills"],
        json!(["SQL", "Python", "dbt", "PostgreSQL"])
    );
    stack.stop().await;
}

#[tokio::test]
async fn bad_rows_are_reported_and_the_good_ones_imported() {
    let (stack, _) = setup().await;
    let base = stack.web_crm_url.clone();
    let file = temp_file(
        "mixed.csv",
        "Имя;Email\nАнна Первая;anna@mail.ru\n;nameless@mail.ru\nБорис Второй;плохой\nВера Третья;vera@mail.ru\n",
    );

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        pick(&tab, &base, &file);
        wait_until(&tab, "mapping", &exists_js("[data-testid=import-table]"));
        click(&tab, "[data-testid=import-start]");
        wait_until(&tab, "done", &exists_js("[data-testid=import-done]"));
        assert_eq!(text(&tab, "[data-testid=sum-created]").trim(), "2");
        assert_eq!(text(&tab, "[data-testid=sum-rejected]").trim(), "2");
        let rejected = text(&tab, "[data-testid=rejected-list]");
        assert!(rejected.contains("Строка 3"), "{rejected}");
        assert!(rejected.contains("Строка 4"), "{rejected}");
        assert!(rejected.contains("email"), "{rejected}");
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn the_name_column_is_required_and_files_are_validated() {
    let (stack, _) = setup().await;
    let base = stack.web_crm_url.clone();
    let numbers = temp_file("numbers.csv", "Телефон;Город\n+7 900 000 00 00;Омск\n");
    let wrong = temp_file("notes.docx", "не таблица");

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", OWNER.0, OWNER.1);
        pick(&tab, &base, &numbers);
        wait_until(&tab, "mapping", &exists_js("[data-testid=import-table]"));
        // первая колонка без подходящего заголовка по умолчанию — не имя, а телефон: имени нет
        assert!(text(&tab, "[data-testid=import-hint]").contains("Укажите"));
        assert_eq!(
            eval(&tab, "document.querySelector('[data-testid=import-start]').disabled"),
            Value::Bool(true)
        );
        // назначаем первую колонку именем — кнопка оживает
        eval(
            &tab,
            "(() => { const el = document.querySelector('[data-testid=map-row] select'); el.value = 'name'; \
             el.dispatchEvent(new Event('change', {bubbles: true})); })()",
        );
        wait_until(
            &tab,
            "button enabled",
            "document.querySelector('[data-testid=import-start]').disabled === false",
        );

        // неподдерживаемый файл
        pick(&tab, &base, &wrong);
        wait_until(&tab, "error", &exists_js("[data-testid=import-error]"));
        assert!(text(&tab, "[data-testid=import-error]").contains(".csv"));
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn viewers_have_no_import() {
    let (stack, _) = setup().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        login_crm(&tab, &base, "acme", VIEWER.0, VIEWER.1);
        tab.navigate_to(&format!("{base}/candidates")).unwrap();
        wait_until(&tab, "list", "document.querySelector('main').innerText.includes('Кандидатов пока нет') || !!document.querySelector('table')");
        assert_eq!(eval(&tab, &count_js("[data-testid=import-button]")), 0);
        tab.navigate_to(&format!("{base}/candidates/import")).unwrap();
        wait_until(&tab, "forbidden", &exists_js("[data-testid=import-forbidden]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=file-input]")), 0);
    })
    .await
    .unwrap();
    stack.stop().await;
}
