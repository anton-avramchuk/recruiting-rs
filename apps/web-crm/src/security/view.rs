//! Раздел «Безопасность»: каркас с вкладками и список пользователей.
use grid_contracts::Operator;
use leptos::prelude::*;
use leptos_router::components::A;
use ui_kit::Reply;
use ui_kit::{
    EmptyState, PageHeader, SessionCtx,
    grid::{Column, Grid},
};

use super::{
    role_label,
    server::{RoleRow, UserRow, list_roles, load_users},
};
use crate::candidates::list::is_owner;

/// Вкладки раздела; новые вкладки добавляются сюда.
#[component]
pub fn SecurityTabs() -> impl IntoView {
    let location = leptos_router::hooks::use_location();
    let users_tab = move || !location.pathname.get().starts_with("/security/roles");
    view! {
        <nav class="tabs" aria-label="Вкладки раздела «Безопасность»" data-testid="security-tabs">
            <a
                href="/security"
                class="tab"
                data-testid="tab-users"
                aria-current=move || users_tab().then_some("page")
            >
                "Пользователи"
            </a>
            <a
                href="/security/roles"
                class="tab"
                data-testid="tab-roles"
                aria-current=move || (!users_tab()).then_some("page")
            >
                "Роли"
            </a>
        </nav>
    }
}

/// Страница раздела: заголовок, вкладки и содержимое только для владельца.
#[component]
pub fn SecurityShell(
    title: &'static str,
    /// Кнопки справа в заголовке.
    #[prop(optional)]
    actions: Option<ViewFn>,
    children: ChildrenFn,
) -> impl IntoView {
    let session = expect_context::<SessionCtx>();
    let owner = Signal::derive(move || is_owner(session));
    view! {
        <PageHeader title=title>{actions.map(|a| a.run())}</PageHeader>
        <Show
            when=move || owner.get()
            fallback=|| {
                view! {
                    <div data-testid="security-denied">
                        <EmptyState title="Нет доступа" hint="Управлять пользователями может только владелец организации." />
                    </div>
                }
            }
        >
            <SecurityTabs />
            {children()}
        </Show>
    }
}

/// Подпись роли пользователя в списке.
pub fn roles_label(user: &UserRow) -> String {
    if let Some(name) = &user.role_name {
        return name.clone();
    }
    match user.roles.as_slice() {
        [one] => role_label(one).to_string(),
        _ => "Несколько ролей".into(),
    }
}

#[component]
pub fn SecurityPage() -> impl IntoView {
    view! {
        <SecurityShell
            title="Безопасность"
            actions=ViewFn::from(|| {
                view! {
                    <A href="/security/users/new" attr:class="btn btn--primary" attr:data-testid="user-new">
                        "Добавить пользователя"
                    </A>
                }
            })
        >
            <UsersList />
        </SecurityShell>
    }
}

/// Адрес страницы пользователя.
fn user_href(user: &UserRow) -> String {
    format!("/security/users/{}", user.id)
}

fn user_columns(me: String, role_options: Signal<Vec<(String, String)>>) -> Vec<Column<UserRow>> {
    vec![
        Column::new("email", "Пользователь", move |u: &UserRow| {
            let title = if u.display_name.trim().is_empty() {
                u.email.clone()
            } else {
                u.display_name.clone()
            };
            let you = (u.id == me).then_some(" (вы)");
            view! {
                <span data-testid="user-title" data-email=u.email.clone()>{title} {you}</span>
                <span class="cell-sub">{u.email.clone()}</span>
            }
            .into_any()
        })
        .sortable(),
        Column::new("role", "Роль", |u: &UserRow| {
            view! { <span class="chip" data-testid="user-role-label">{roles_label(u)}</span> }.into_any()
        })
        .filter_select(role_options, Operator::Contains),
        Column::new("status", "Статус", |u: &UserRow| {
            let blocked = u.status == "blocked";
            view! {
                <span class=if blocked { "badge badge--off" } else { "badge" } data-testid="user-status">
                    {if blocked { "Заблокирован" } else { "Активен" }}
                </span>
            }
            .into_any()
        })
        .sortable()
        .filter_select(
            Signal::stored(vec![
                ("active".to_string(), "Активен".to_string()),
                ("blocked".to_string(), "Заблокирован".to_string()),
            ]),
            Operator::Eq,
        ),
        Column::new("created_at", "Создан", |u: &UserRow| {
            let date = u
                .created_at
                .as_deref()
                .and_then(|t| t.get(..10))
                .unwrap_or("")
                .to_string();
            view! { <span class="muted">{date}</span> }.into_any()
        })
        .sortable(),
    ]
}

#[component]
fn UsersList() -> impl IntoView {
    let session = expect_context::<SessionCtx>();
    let me = session
        .current()
        .and_then(Result::ok)
        .and_then(|s| s.user)
        .map(|u| u.id)
        .unwrap_or_default();
    let href: fn(&UserRow) -> String = user_href;
    // варианты фильтра по роли: встроенные отбираются по коду, собственные - по идентификатору;
    // пока список ролей не загрузился, предлагаются встроенные
    let roles = LocalResource::new(list_roles);
    let role_options = Signal::derive(move || {
        roles
            .get()
            .and_then(Result::ok)
            .and_then(Reply::ok)
            .unwrap_or_else(built_in_roles)
            .into_iter()
            .map(|r| (r.id, r.name))
            .collect::<Vec<_>>()
    });
    view! {
        <Grid
            columns=user_columns(me, role_options)
            load=load_users
            row_href=Some(href)
            search_fields=vec!["email", "display_name"]
            search_placeholder="Поиск по имени и email"
            empty_title="Пользователей пока нет"
            empty_hint="Добавьте первого пользователя."
        />
    }
}

/// Встроенные роли на случай, когда список ролей не загрузился.
pub fn built_in_roles() -> Vec<RoleRow> {
    [
        ("owner", "Владелец"),
        ("recruiter", "Рекрутёр"),
        ("viewer", "Наблюдатель"),
    ]
    .into_iter()
    .map(|(id, name)| RoleRow {
        id: id.into(),
        kind: "builtin".into(),
        name: name.into(),
        permissions: Vec::new(),
        users: 0,
    })
    .collect()
}
