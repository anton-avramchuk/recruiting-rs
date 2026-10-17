//! Экран «Безопасность»: управление пользователями организации в браузере.
mod common;

use std::time::Duration;

use common::*;
use e2e::Stack;
use headless_chrome::browser::tab::ModifierKey;

const OWNER: (&str, &str) = ("boss@acme.io", "boss-password-1");
const VIEWER: (&str, &str) = ("viewer@acme.io", "viewer-password-1");
const NEW: &str = "new@acme.io";

fn text(tab: &Tab, selector: &str) -> String {
    eval_str(
        tab,
        &format!("(document.querySelector({selector:?})||{{}}).innerText||''"),
    )
}

/// Выбирает в `select` вариант, текст которого содержит `label`, и сообщает фреймворку.
fn choose(tab: &Tab, selector: &str, label: &str) {
    eval(
        tab,
        &format!(
            "(() => {{ const s = document.querySelector({selector:?}); \
             const o = Array.from(s.options).find(o => o.textContent.includes({label:?})); \
             s.value = o.value; s.dispatchEvent(new Event('change', {{bubbles: true}})); }})()"
        ),
    );
}

/// Часть строки грида, в которой есть пользователь с этим email.
fn path_with_query(tab: &Tab) -> String {
    eval_str(tab, "location.pathname + location.search")
}

fn row(email: &str, part: &str) -> String {
    format!("tr[data-testid=grid-row]:has([data-email='{email}']) {part}")
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

fn open_user(tab: &Tab, email: &str) {
    click(tab, &format!("[data-email='{email}']"));
    wait_until(
        tab,
        "user page",
        &exists_js("[data-testid=user-role-select]"),
    );
}

fn back_to_users(tab: &Tab) {
    click(tab, "[data-testid=back-to-users]");
    wait_until(tab, "users list", &exists_js("[data-testid=grid-row]"));
}

fn error_contains(part: &str) -> String {
    format!(
        "(document.querySelector('[data-testid=users-error]')||{{}}).innerText?.includes({part:?})"
    )
}

fn ok_contains(part: &str) -> String {
    format!(
        "(document.querySelector('[data-testid=users-ok]')||{{}}).innerText?.includes({part:?})"
    )
}

#[tokio::test]
async fn the_owner_manages_users_end_to_end() {
    let stack = world().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);
        set_marker(&tab);

        click(&tab, "nav a[href='/security']");
        wait_until(&tab, "security page", &exists_js("[data-testid=grid-row]"));
        wait_count(&tab, "two users", "[data-testid=grid-row]", 2);
        assert_eq!(eval(&tab, &count_js("[data-testid=security-tabs]")), 1);

        // создание на отдельной странице
        click(&tab, "[data-testid=user-new]");
        wait_until(&tab, "new user page", &exists_js("[data-testid=user-email]"));
        assert_eq!(path_of(&tab), "/security/users/new");
        type_into(&tab, "[data-testid=user-email]", NEW);
        type_into(&tab, "[data-testid=user-name]", "Новый");
        type_into(&tab, "[data-testid=user-password]", "new-password-1");
        click(&tab, "[data-testid=user-add]");
        wait_until(&tab, "back on list", &exists_js("[data-testid=grid]"));
        wait_count(&tab, "three users", "[data-testid=grid-row]", 3);
        assert!(text(&tab, &row(NEW, "[data-testid=user-title]")).contains("Новый"));

        // занятый email и слабый пароль: ошибка у формы, остаёмся на странице
        click(&tab, "[data-testid=user-new]");
        wait_until(&tab, "new user page", &exists_js("[data-testid=user-email]"));
        type_into(&tab, "[data-testid=user-email]", NEW);
        type_into(&tab, "[data-testid=user-password]", "new-password-1");
        click(&tab, "[data-testid=user-add]");
        wait_until(&tab, "duplicate error", &error_contains("уже есть"));
        replace_text(&tab, "[data-testid=user-email]", "weak@acme.io");
        replace_text(&tab, "[data-testid=user-password]", "short");
        click(&tab, "[data-testid=user-add]");
        wait_until(&tab, "weak password error", &error_contains("8"));
        assert_eq!(path_of(&tab), "/security/users/new");
        back_to_users(&tab);
        assert_eq!(eval(&tab, &count_js("[data-testid=grid-row]")), 3);

        // смена роли на странице пользователя
        open_user(&tab, NEW);
        choose(&tab, "[data-testid=user-role-select]", "Наблюдатель");
        click(&tab, "[data-testid=user-role-save]");
        wait_until(&tab, "role saved", &ok_contains("Роль"));
        wait_until(
            &tab,
            "heading shows the role",
            "document.querySelector('[data-testid=user-heading]').innerText.includes('Наблюдатель')",
        );
        back_to_users(&tab);
        assert_eq!(
            text(&tab, &row(NEW, "[data-testid=user-role-label]")),
            "Наблюдатель"
        );

        // единственный владелец не может понизить или заблокировать себя
        open_user(&tab, OWNER.0);
        choose(&tab, "[data-testid=user-role-select]", "Рекрутёр");
        click(&tab, "[data-testid=user-role-save]");
        wait_until(&tab, "owner guard", &error_contains("владелец"));
        wait_until(
            &tab,
            "owner role restored",
            "document.querySelector('[data-testid=user-role-select]').value === 'owner'",
        );
        click(&tab, "[data-testid=user-toggle]");
        wait_until(&tab, "self block refused", &error_contains("самого себя"));
        assert_eq!(text(&tab, "[data-testid=user-status]"), "Активен");
        back_to_users(&tab);

        // блокировка и разблокировка
        open_user(&tab, NEW);
        click(&tab, "[data-testid=user-toggle]");
        wait_until(
            &tab,
            "blocked",
            "(document.querySelector('[data-testid=user-status]')||{}).innerText === 'Заблокирован'",
        );
        click(&tab, "[data-testid=user-toggle]");
        wait_until(
            &tab,
            "unblocked",
            "(document.querySelector('[data-testid=user-status]')||{}).innerText === 'Активен'",
        );

        // смена имени
        replace_text(&tab, "[data-testid=user-name-input]", "Другое имя");
        click(&tab, "[data-testid=user-name-save]");
        wait_until(&tab, "name saved", &ok_contains("Имя"));

        // сброс пароля: слабый, затем нормальный
        type_into(&tab, "[data-testid=password-input]", "short");
        click(&tab, "[data-testid=password-save]");
        wait_until(&tab, "weak reset refused", &error_contains("8"));
        replace_text(&tab, "[data-testid=password-input]", "fresh-password-1");
        click(&tab, "[data-testid=password-save]");
        wait_until(&tab, "password changed", &ok_contains("Пароль изменён"));
        back_to_users(&tab);
        assert!(text(&tab, &row(NEW, "[data-testid=user-title]")).contains("Другое имя"));

        // владельцу палитра предлагает раздел
        tab.press_key_with_modifiers("k", Some(&[ModifierKey::Ctrl]))
            .unwrap();
        wait_until(&tab, "palette", &exists_js("[data-testid=palette-input]"));
        type_into(&tab, "[data-testid=palette-input]", "безопас");
        wait_until(&tab, "security offered", &exists_js("[data-testid=palette-item]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=palette-item]")), 1);

        // новый пароль работает в отдельном браузере; наблюдатель не видит раздел
        let other = launch();
        let tab2 = other.new_tab().unwrap();
        login_crm(&tab2, &base, "acme", NEW, "fresh-password-1");
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(eval(&tab2, &count_js("nav a[href='/security']")), 0);
    })
    .await
    .unwrap();
    stack.stop().await;
}

fn info(tab: &Tab) -> String {
    text(tab, "[data-testid=grid-page-info]")
}

fn wait_info(tab: &Tab, expected: &str) {
    wait_until(
        tab,
        &format!("pager shows {expected}"),
        &format!(
            "(document.querySelector('[data-testid=grid-page-info]')||{{}}).innerText === {expected:?}"
        ),
    );
}

#[tokio::test]
async fn the_users_grid_pages_sorts_filters_and_keeps_its_state_in_the_address() {
    let stack = world().await;
    let owner = stack.crm_client("acme", OWNER.0, OWNER.1).await;
    for i in 0..22 {
        let role = if i % 2 == 0 { "recruiter" } else { "viewer" };
        stack
            .add_user(
                &owner,
                &format!("user{i:02}@acme.io"),
                "long-enough-1",
                role,
            )
            .await;
    }
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);
        click(&tab, "nav a[href='/security']");
        wait_until(&tab, "grid", &exists_js("[data-testid=grid-row]"));

        // 24 пользователя: владелец, наблюдатель и 22 созданных
        wait_info(&tab, "1–20 из 24");
        assert_eq!(eval(&tab, &count_js("[data-testid=grid-row]")), 20);
        click(&tab, "[data-testid=grid-next]");
        wait_info(&tab, "21–24 из 24");
        assert!(path_with_query(&tab).contains("page=2"));
        click(&tab, "[data-testid=grid-prev]");
        wait_info(&tab, "1–20 из 24");

        // сортировка по заголовку: по возрастанию, по убыванию, без сортировки
        click(&tab, "[data-testid=grid-sort-email]");
        wait_until(&tab, "ascending", "document.querySelector('th[aria-sort=ascending]') !== null");
        assert!(text(&tab, "[data-testid=grid-row]").contains("boss@acme.io"));
        click(&tab, "[data-testid=grid-sort-email]");
        wait_until(&tab, "descending", "document.querySelector('th[aria-sort=descending]') !== null");
        wait_until(
            &tab,
            "viewer first",
            "(document.querySelector('[data-testid=grid-row]')||{}).innerText?.includes('viewer@acme.io')",
        );

        // фильтр по роли
        choose(&tab, "[data-testid=grid-filter-role]", "Рекрутёр");
        wait_info(&tab, "1–11 из 11");
        assert!(path_with_query(&tab).contains("f.role=recruiter"));

        // адрес хранит представление: после перезагрузки оно то же
        let url = tab.get_url();
        tab.navigate_to(&url).unwrap();
        std::thread::sleep(Duration::from_millis(1500));
        wait_info(&tab, "1–11 из 11");
        assert_eq!(
            eval_str(&tab, "document.querySelector('[data-testid=grid-filter-role]').value"),
            "recruiter"
        );

        // поиск внутри фильтра
        type_into(&tab, "[data-testid=grid-search]", "user04");
        wait_info(&tab, "1–1 из 1");
        assert!(path_with_query(&tab).contains("q=user04"));

        // ничего не найдено
        replace_text(&tab, "[data-testid=grid-search]", "no-such-person");
        wait_until(&tab, "empty state", &exists_js("[data-testid=grid-empty]"));
        assert_eq!(info(&tab), "Ничего нет");

        // сброс возвращает полный список
        click(&tab, "[data-testid=grid-reset]");
        wait_info(&tab, "1–20 из 24");
        assert_eq!(eval(&tab, &count_js("[data-testid=grid-reset]")), 0);

        // размер страницы
        choose(&tab, "[data-testid=grid-page-size]", "50");
        wait_info(&tab, "1–24 из 24");
        assert_eq!(eval(&tab, &count_js("[data-testid=grid-row]")), 24);
    })
    .await
    .unwrap();
    stack.stop().await;
}

#[tokio::test]
async fn other_roles_do_not_see_the_security_section() {
    let stack = world().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, VIEWER);
        assert_eq!(eval(&tab, &count_js("nav a[href='/security']")), 0);

        tab.navigate_to(&format!("{base}/security")).unwrap();
        wait_until(&tab, "denied", &exists_js("[data-testid=security-denied]"));
        assert_eq!(eval(&tab, &count_js("[data-testid=grid-row]")), 0);

        // палитра не предлагает раздел
        std::thread::sleep(Duration::from_millis(1500)); // гидратация после перезагрузки
        tab.press_key_with_modifiers("k", Some(&[ModifierKey::Ctrl]))
            .unwrap();
        wait_until(&tab, "palette", &exists_js("[data-testid=palette-input]"));
        type_into(&tab, "[data-testid=palette-input]", "безопас");
        wait_until(
            &tab,
            "nothing offered",
            &exists_js("[data-testid=palette-empty]"),
        );
    })
    .await
    .unwrap();
    stack.stop().await;
}

/// Ждёт, пока в выбор загрузится вариант, и выбирает его.
fn choose_ready(tab: &Tab, selector: &str, label: &str) {
    wait_until(
        tab,
        &format!("option {label} in {selector}"),
        &format!(
            "Array.from(document.querySelectorAll({selector:?} + ' option')).some(o => o.textContent.includes({label:?}))"
        ),
    );
    choose(tab, selector, label);
}

fn roles_error_contains(part: &str) -> String {
    format!(
        "(document.querySelector('[data-testid=roles-error]')||{{}}).innerText?.includes({part:?})"
    )
}

#[tokio::test]
async fn the_owner_manages_custom_roles_and_their_users() {
    let stack = world().await;
    let base = stack.web_crm_url.clone();

    tokio::task::spawn_blocking(move || {
        let browser = launch();
        let tab = browser.new_tab().unwrap();
        sign_in(&tab, &base, OWNER);

        // вкладка «Роли»: три встроенные роли
        click(&tab, "nav a[href='/security']");
        wait_until(&tab, "security page", &exists_js("[data-testid=tab-roles]"));
        click(&tab, "[data-testid=tab-roles]");
        wait_count(&tab, "built-in roles", "[data-testid=role-row]", 3);
        assert_eq!(eval(&tab, &count_js("[data-testid=role-row][data-kind=builtin]")), 3);

        // создание роли «Стажёр»: чтение и запись (запись включает чтение)
        click(&tab, "[data-testid=role-new]");
        wait_until(&tab, "new role page", &exists_js("[data-testid=role-name-input]"));
        type_into(&tab, "[data-testid=role-name-input]", "Стажёр");
        click(&tab, "[data-testid=perm-write]");
        click(&tab, "[data-testid=role-save]");
        wait_count(&tab, "four roles", "[data-testid=role-row]", 4);
        let role_text = text(&tab, "[data-testid=role-row][data-kind=custom]");
        assert!(role_text.contains("Стажёр") && role_text.contains("Чтение") && role_text.contains("Запись"), "{role_text}");
        assert!(!role_text.contains("Удаление"), "{role_text}");

        // занятое и зарезервированное названия
        click(&tab, "[data-testid=role-new]");
        wait_until(&tab, "new role page", &exists_js("[data-testid=role-name-input]"));
        type_into(&tab, "[data-testid=role-name-input]", "стажёр");
        click(&tab, "[data-testid=role-save]");
        wait_until(&tab, "duplicate", &roles_error_contains("уже есть"));
        replace_text(&tab, "[data-testid=role-name-input]", "Владелец");
        click(&tab, "[data-testid=role-save]");
        wait_until(&tab, "reserved", &roles_error_contains("встроенной"));
        click(&tab, "[data-testid=back-to-roles]");
        wait_count(&tab, "still four", "[data-testid=role-row]", 4);

        // пользователь с этой ролью
        click(&tab, "[data-testid=tab-users]");
        wait_until(&tab, "users", &exists_js("[data-testid=user-new]"));
        click(&tab, "[data-testid=user-new]");
        wait_until(&tab, "new user", &exists_js("[data-testid=user-email]"));
        type_into(&tab, "[data-testid=user-email]", "intern@acme.io");
        type_into(&tab, "[data-testid=user-password]", "intern-password-1");
        choose_ready(&tab, "[data-testid=user-role]", "Стажёр");
        click(&tab, "[data-testid=user-add]");
        wait_until(&tab, "back on list", &exists_js("[data-testid=grid-row]"));
        wait_until(
            &tab,
            "role name in the list",
            &format!(
                "(document.querySelector({:?})||{{}}).innerText === 'Стажёр'",
                row("intern@acme.io", "[data-testid=user-role-label]")
            ),
        );

        // стажёр видит кнопки записи, но не раздел «Безопасность»
        let other = launch();
        let tab2 = other.new_tab().unwrap();
        login_crm(&tab2, &base, "acme", "intern@acme.io", "intern-password-1");
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(eval(&tab2, &count_js("nav a[href='/security']")), 0);
        click(&tab2, "nav a[href='/candidates']");
        wait_until(&tab2, "candidates", &exists_js("[data-testid=import-button]"));

        // удалить назначенную роль нельзя
        click(&tab, "[data-testid=tab-roles]");
        wait_count(&tab, "roles", "[data-testid=role-row]", 4);
        click(&tab, "[data-testid=role-row][data-kind=custom]");
        wait_until(&tab, "role page", &exists_js("[data-testid=role-delete]"));
        assert!(text(&tab, "[data-testid=role-heading]").contains('1'));
        click(&tab, "[data-testid=role-delete]");
        wait_until(&tab, "assigned", &roles_error_contains("назначена"));

        // без права записи кнопок записи нет: оставляем только чтение
        click(&tab, "[data-testid=perm-write]");
        click(&tab, "[data-testid=role-save]");
        wait_until(
            &tab,
            "role saved",
            "(document.querySelector('[data-testid=roles-ok]')||{}).innerText?.includes('сохранена')",
        );
        assert_eq!(eval(&tab, &count_js("[data-testid=perm-write]:checked")), 0);

        // смена пользователю роли на встроенную и удаление свободной роли
        click(&tab, "[data-testid=tab-users]");
        wait_until(&tab, "users", &exists_js("[data-testid=grid-row]"));
        click(&tab, &row("intern@acme.io", "[data-email]"));
        wait_until(&tab, "user page", &exists_js("[data-testid=user-role-select]"));
        choose_ready(&tab, "[data-testid=user-role-select]", "Наблюдатель");
        click(&tab, "[data-testid=user-role-save]");
        wait_until(&tab, "role changed", &ok_contains("Роль"));
        click(&tab, "[data-testid=tab-roles]");
        wait_count(&tab, "roles", "[data-testid=role-row]", 4);
        click(&tab, "[data-testid=role-row][data-kind=custom]");
        wait_until(&tab, "role page", &exists_js("[data-testid=role-delete]"));
        click(&tab, "[data-testid=role-delete]");
        wait_count(&tab, "role removed", "[data-testid=role-row]", 3);
    })
    .await
    .unwrap();
    stack.stop().await;
}
