//! Раздел «Безопасность»: каркас с вкладками и список пользователей.
use leptos::prelude::*;
use leptos_router::components::A;
use ui_kit::{EmptyState, Notice, NoticeKind, PageHeader, SessionCtx};

use super::{
    role_label,
    server::{UserRow, list_users},
};
use crate::{candidates::list::can_delete, funnel::server::Reply};

/// Вкладки раздела; новые вкладки добавляются сюда.
#[component]
pub fn SecurityTabs() -> impl IntoView {
    view! {
        <nav class="tabs" aria-label="Вкладки раздела «Безопасность»" data-testid="security-tabs">
            <A href="/security" attr:class="tab">"Пользователи"</A>
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
    let owner = Signal::derive(move || can_delete(session));
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

#[component]
fn UsersList() -> impl IntoView {
    let session = expect_context::<SessionCtx>();
    let me = move || {
        session
            .current()
            .and_then(Result::ok)
            .and_then(|s| s.user)
            .map(|u| u.id)
            .unwrap_or_default()
    };
    let users = Resource::new(|| (), |_| list_users());
    view! {
        <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
            {move || {
                users
                    .get()
                    .map(|result| match result {
                        Ok(Reply::Ok(list)) if list.is_empty() => {
                            view! { <EmptyState title="Пользователей пока нет" hint="Добавьте первого пользователя." /> }
                                .into_any()
                        }
                        Ok(Reply::Ok(list)) => {
                            let me = me();
                            view! {
                                <div class="card list-card" data-testid="users">
                                    {list
                                        .into_iter()
                                        .map(|u| {
                                            let is_me = u.id == me;
                                            view! { <UserRowView user=u is_me /> }
                                        })
                                        .collect_view()}
                                </div>
                            }
                                .into_any()
                        }
                        Ok(Reply::Forbidden) => {
                            view! { <EmptyState title="Нет доступа" hint="Управлять пользователями может только владелец организации." /> }
                                .into_any()
                        }
                        _ => {
                            view! { <Notice kind=NoticeKind::Error>"Не удалось загрузить пользователей. Попробуйте позже."</Notice> }
                                .into_any()
                        }
                    })
            }}
        </Transition>
    }
}

#[component]
fn UserRowView(user: UserRow, is_me: bool) -> impl IntoView {
    let blocked = user.status == "blocked";
    let title = if user.display_name.trim().is_empty() {
        user.email.clone()
    } else {
        user.display_name.clone()
    };
    let role = roles_label(&user);
    let (email, email_attr) = (user.email.clone(), user.email.clone());
    view! {
        <A href=format!("/security/users/{}", user.id) attr:class="qrow" attr:data-testid="user-row" attr:data-email=email_attr>
            <span class="qrow__main">
                <span class="nm" data-testid="user-title">{title} {is_me.then_some(" (вы)")}</span>
                <span class="sub">{email}</span>
            </span>
            <span class="chip" data-testid="user-role-label">{role}</span>
            <span class=if blocked { "badge badge--off" } else { "badge" } data-testid="user-status">
                {if blocked { "Заблокирован" } else { "Активен" }}
            </span>
        </A>
    }
}
