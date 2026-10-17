//! Комментарии к заказчикам, вакансиям и кандидатам в браузере.
mod common;

use std::time::Duration;

use common::*;
use e2e::{CrmClient, Stack};
use serde_json::{Value, json};

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");
const RECRUITER: (&str, &str) = ("rec@acme.io", "rec-password-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-password-1");

struct World {
    stack: Stack,
    client: String,
    person: String,
}

fn text(tab: &Tab, selector: &str) -> String {
    eval_str(
        tab,
        &format!("(document.querySelector({selector:?})||{{}}).innerText||''"),
    )
}

async fn world() -> World {
    let stack = Stack::start_with_web().await;
    stack.seed_tenant("acme", OWNER.0, OWNER.1).await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    stack
        .add_user(&owner, RECRUITER.0, RECRUITER.1, "recruiter")
        .await;
    stack.add_user(&owner, VIEWER.0, VIEWER.1, "viewer").await;
    let client = stack.add_client(&owner, "Acme Corp").await;
    let vacancy = created(
        &owner,
        "/api/vacancies",
        json!({"title": "Senior Go", "client_id": client}),
    )
    .await;
    let person = created(
        &owner,
        "/api/persons",
        json!({"full_name": "Анна Смирнова", "source": "hh.ru"}),
    )
    .await;
    created(
        &owner,
        "/api/applications",
        json!({"person_id": person["id"], "vacancy_id": vacancy["id"]}),
    )
    .await;
    World {
        client,
        person: person["id"].as_str().unwrap().to_string(),
        stack,
    }
}

async fn created(owner: &CrmClient, path: &str, body: Value) -> Value {
    let res = owner.post(path, body).await;
    assert_eq!(res.status(), 201, "{path}");
    res.json().await.unwrap()
}

fn sign_in(tab: &Tab, base: &str, user: (&str, &str)) {
    login_crm(tab, base, "acme", user.0, user.1);
    std::thread::sleep(Duration::from_millis(1500)); // гидратация
}

fn open_client(tab: &Tab, base: &str, id: &str) {
    tab.navigate_to(&format!("{base}/clients/{id}")).unwrap();
    wait_until(tab, "comments block", &exists_js("[data-testid=comments]"));
    wait_until(
        tab,
        "feed loaded",
        "!document.querySelector('[data-testid=comments]').innerText.includes('Загрузка')",
    );
}

fn add_comment(tab: &Tab, body: &str) {
    type_into(tab, "[data-testid=comment-text]", body);
    click(tab, "[data-testid=comment-submit]");
}

#[tokio::test]
async fn adding_editing_and_deleting_a_comment_on_a_client() {
    let w = world().await;
    let base = w.stack.web_crm_url.clone();
    let client = w.client.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);
        open_client(&tab, &base, &client);
        set_marker(&tab);

        assert!(text(&tab, "[data-testid=comments-empty]").contains("пока нет"));
        add_comment(&tab, "Созвон по понедельникам");
        wait_until(&tab, "comment shown", &exists_js("[data-testid=comment]"));
        assert!(text(&tab, "[data-testid=comment-body]").contains("Созвон по понедельникам"));
        let author = text(&tab, "[data-testid=comment-author]");
        assert!(
            !author.trim().is_empty() && author.trim() != "Пользователь",
            "{author}"
        );
        assert_eq!(
            eval_str(
                &tab,
                "document.querySelector('[data-testid=comment-text]').value"
            ),
            ""
        );
        assert!(marker_alive(&tab), "no reload");

        // правка помечается «изменено»
        click(&tab, "[data-testid=comment-edit]");
        wait_until(
            &tab,
            "edit form",
            &exists_js("[data-testid=comment-edit-text]"),
        );
        replace_text(
            &tab,
            "[data-testid=comment-edit-text]",
            "Созвон по вторникам",
        );
        click(&tab, "[data-testid=comment-save]");
        wait_until(&tab, "edited", &exists_js("[data-testid=comment-edited]"));
        assert!(text(&tab, "[data-testid=comment-body]").contains("по вторникам"));

        // отмена правки не меняет текст
        click(&tab, "[data-testid=comment-edit]");
        replace_text(&tab, "[data-testid=comment-edit-text]", "не сохранять");
        click(&tab, "[data-testid=comment-cancel]");
        wait_until(
            &tab,
            "form closed",
            &gone_js("[data-testid=comment-edit-text]"),
        );
        assert!(text(&tab, "[data-testid=comment-body]").contains("по вторникам"));

        // удаление
        click(&tab, "[data-testid=comment-delete]");
        wait_until(
            &tab,
            "empty again",
            &exists_js("[data-testid=comments-empty]"),
        );

        // пустой текст не отправляется
        click(&tab, "[data-testid=comment-submit]");
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(eval(&tab, &count_js("[data-testid=comment]")), 0);
    })
    .await
    .unwrap();
    w.stack.stop().await;
}

#[tokio::test]
async fn only_the_author_edits_and_the_owner_also_deletes() {
    let w = world().await;
    let base = w.stack.web_crm_url.clone();
    let client = w.client.clone();

    tokio::task::spawn_blocking(move || {
        // у вкладок одного браузера общие cookie, поэтому у каждого пользователя свой браузер
        let rec_browser = launch();
        let owner_browser = launch();

        // рекрутёр пишет комментарий
        let rec = rec_browser.new_tab().unwrap();
        sign_in(&rec, &base, RECRUITER);
        open_client(&rec, &base, &client);
        add_comment(&rec, "Комментарий рекрутёра");
        wait_until(&rec, "shown", &exists_js("[data-testid=comment]"));
        assert_eq!(eval(&rec, &count_js("[data-testid=comment-edit]")), 1);
        assert_eq!(eval(&rec, &count_js("[data-testid=comment-delete]")), 1);

        // владелец видит его, удаляет, но править не может
        let owner = owner_browser.new_tab().unwrap();
        sign_in(&owner, &base, OWNER);
        open_client(&owner, &base, &client);
        wait_until(&owner, "visible", &exists_js("[data-testid=comment]"));
        assert!(text(&owner, "[data-testid=comment-body]").contains("рекрутёра"));
        assert_eq!(eval(&owner, &count_js("[data-testid=comment-edit]")), 0);
        assert_eq!(eval(&owner, &count_js("[data-testid=comment-delete]")), 1);

        // владелец отвечает; рекрутёр на чужом комментарии кнопок не видит
        add_comment(&owner, "Ответ владельца");
        wait_until(
            &owner,
            "two",
            "document.querySelectorAll('[data-testid=comment]').length === 2",
        );
        rec.navigate_to(&format!("{base}/clients/{client}"))
            .unwrap();
        wait_until(
            &rec,
            "two for recruiter",
            "document.querySelectorAll('[data-testid=comment]').length === 2",
        );
        assert_eq!(
            eval(&rec, &count_js("[data-testid=comment-edit]")),
            1,
            "only their own"
        );
        assert_eq!(
            eval(&rec, &count_js("[data-testid=comment-delete]")),
            1,
            "only their own"
        );

        click(&owner, "[data-testid=comment-delete]"); // первый - комментарий рекрутёра
        wait_until(
            &owner,
            "one left",
            "document.querySelectorAll('[data-testid=comment]').length === 1",
        );
        assert!(text(&owner, "[data-testid=comment-body]").contains("Ответ владельца"));
    })
    .await
    .unwrap();
    w.stack.stop().await;
}

#[tokio::test]
async fn a_viewer_reads_comments_but_has_no_form_or_buttons() {
    let w = world().await;
    let base = w.stack.web_crm_url.clone();
    let client = w.client.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let rec = browser.new_tab().unwrap();
        sign_in(&rec, &base, RECRUITER);
        open_client(&rec, &base, &client);
        add_comment(&rec, "Видно всем");
        wait_until(&rec, "shown", &exists_js("[data-testid=comment]"));

        let viewer = browser.new_tab().unwrap();
        sign_in(&viewer, &base, VIEWER);
        open_client(&viewer, &base, &client);
        wait_until(&viewer, "visible", &exists_js("[data-testid=comment]"));
        assert!(text(&viewer, "[data-testid=comment-body]").contains("Видно всем"));
        for hidden in [
            "comment-form",
            "comment-edit",
            "comment-delete",
            "comment-text",
        ] {
            assert_eq!(
                eval(&viewer, &count_js(&format!("[data-testid={hidden}]"))),
                0,
                "{hidden}"
            );
        }
    })
    .await
    .unwrap();
    w.stack.stop().await;
}

#[tokio::test]
async fn comments_live_on_vacancies_candidates_and_in_the_panel() {
    let w = world().await;
    let base = w.stack.web_crm_url.clone();
    let person = w.person.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);

        // вакансия
        click(&tab, "nav a[href='/vacancies']");
        wait_until(&tab, "vacancy block", &exists_js("[data-testid=comments]"));
        wait_until(&tab, "loaded", &exists_js("[data-testid=comments-empty]"));
        add_comment(&tab, "По вакансии");
        wait_until(&tab, "vacancy comment", &exists_js("[data-testid=comment]"));

        // карточка кандидата
        tab.navigate_to(&format!("{base}/candidates/{person}")).unwrap();
        wait_until(&tab, "person block", &exists_js("[data-testid=comments]"));
        wait_until(&tab, "empty", &exists_js("[data-testid=comments-empty]"));
        add_comment(&tab, "Перезвонить в пятницу");
        wait_until(&tab, "person comment", &exists_js("[data-testid=comment]"));

        // тот же человек в панели откликов: видна та же лента
        tab.navigate_to(&format!("{base}/applications")).unwrap();
        wait_until(&tab, "row", &exists_js("[data-testid=open-app]"));
        std::thread::sleep(Duration::from_millis(1200));
        click(&tab, "[data-testid=open-app]");
        wait_until(&tab, "panel", &exists_js("[data-testid=panel] [data-testid=comments]"));
        wait_until(
            &tab,
            "same feed in the panel",
            "(document.querySelector('[data-testid=panel] [data-testid=comment-body]')||{innerText:''}).innerText.includes('Перезвонить в пятницу')",
        );
        // а комментарий вакансии в ленту человека не попал
        assert!(!text(&tab, "[data-testid=panel] [data-testid=comments]").contains("По вакансии"));
    })
    .await
    .unwrap();
    w.stack.stop().await;
}

#[tokio::test]
async fn an_unavailable_comment_service_leaves_the_page_working() {
    let w = world().await;
    let base = w.stack.web_crm_url.clone();
    let client = w.client.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);
        open_client(&tab, &base, &client);

        // сервис комментариев «упал»: запросы к /comments отклоняются
        eval(
            &tab,
            "(() => { window.__fetch = window.fetch; \
             window.fetch = (u, ...a) => ((typeof u === 'string' ? u : (u && u.url) || '').includes('/comments')) \
               ? Promise.reject(new Error('down')) : window.__fetch(u, ...a); })()",
        );
        click(&tab, "nav a[href='/clients']");
        wait_until(&tab, "clients", &exists_js("[data-testid=grid-row]"));
        click(&tab, "[data-testid=grid-row] [data-testid=row-edit]");
        wait_until(
            &tab,
            "failed block",
            &exists_js("[data-testid=comments-failed]"),
        );
        // остальная карточка на месте
        assert_eq!(text(&tab, "[data-testid=client-title]").trim(), "Acme Corp");

        // сервис вернулся: повтор загружает ленту
        eval(&tab, "window.fetch = window.__fetch");
        click(&tab, "[data-testid=comments-retry]");
        wait_until(
            &tab,
            "recovered",
            &exists_js("[data-testid=comments-empty]"),
        );
    })
    .await
    .unwrap();
    w.stack.stop().await;
}
