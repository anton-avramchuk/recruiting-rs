//! Экран «Безопасность»: список пользователей и управление доступом.
use leptos::prelude::*;
use ui_kit::{EmptyState, Notice, NoticeKind, PageHeader, SessionCtx};

use super::{
    role_label,
    server::{NewUserForm, UserPatch, UserRow, create_user, failure_text, list_users, update_user},
};
use crate::{candidates::list::can_delete, funnel::server::Reply};

const ROLES: [&str; 3] = ["owner", "recruiter", "viewer"];

#[component]
pub fn SecurityPage() -> impl IntoView {
    let session = expect_context::<SessionCtx>();
    let owner = Signal::derive(move || can_delete(session));
    view! {
        <PageHeader title="Безопасность" />
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
            <UsersPanel />
        </Show>
    }
}

#[component]
fn UsersPanel() -> impl IntoView {
    let session = expect_context::<SessionCtx>();
    let me = move || {
        session
            .current()
            .and_then(Result::ok)
            .and_then(|s| s.user)
            .map(|u| u.id)
            .unwrap_or_default()
    };
    let version = RwSignal::new(0u32);
    let error = RwSignal::new(None::<String>);
    let done = RwSignal::new(None::<String>);
    let users = Resource::new(move || version.get(), |_| list_users());

    view! {
        {move || error.get().map(|m| view! { <div data-testid="users-error"><Notice kind=NoticeKind::Error>{m}</Notice></div> })}
        {move || done.get().map(|m| view! { <div data-testid="users-ok"><Notice kind=NoticeKind::Info>{m}</Notice></div> })}
        <CreateForm version error done />
        <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
            {move || {
                users
                    .get()
                    .map(|result| match result {
                        Ok(Reply::Ok(list)) => {
                            let me = me();
                            view! {
                                <div class="card list-card" data-testid="users">
                                    {list
                                        .into_iter()
                                        .map(|u| {
                                            let is_me = u.id == me;
                                            view! { <UserLine user=u is_me version error done /> }
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
fn CreateForm(
    version: RwSignal<u32>,
    error: RwSignal<Option<String>>,
    done: RwSignal<Option<String>>,
) -> impl IntoView {
    let email = RwSignal::new(String::new());
    let name = RwSignal::new(String::new());
    let role = RwSignal::new("recruiter".to_string());
    let password = RwSignal::new(String::new());
    let create = Action::new(move |form: &NewUserForm| {
        let form = form.clone();
        async move { create_user(form).await }
    });
    Effect::new(move |_| {
        if let Some(result) = create.value().get() {
            match result {
                Ok(Reply::Ok(_)) => {
                    error.set(None);
                    done.set(Some("Пользователь добавлен.".into()));
                    for field in [email, name, password] {
                        field.set(String::new());
                    }
                    version.update(|v| *v += 1);
                }
                Ok(other) => {
                    done.set(None);
                    error.set(failure_text(&other));
                }
                Err(_) => {
                    done.set(None);
                    error.set(Some("Не удалось добавить пользователя.".into()));
                }
            }
        }
    });
    view! {
        <form
            class="toolbar"
            method="post"
            onsubmit="return false"
            on:submit=move |ev| {
                ev.prevent_default();
                create
                    .dispatch(NewUserForm {
                        email: email.get(),
                        display_name: name.get(),
                        role: role.get(),
                        password: password.get(),
                    });
            }
        >
            <input
                type="email"
                required
                placeholder="Email"
                aria-label="Email"
                data-testid="user-email"
                prop:value=move || email.get()
                on:input=move |ev| email.set(event_target_value(&ev))
            />
            <input
                type="text"
                placeholder="Имя"
                aria-label="Имя"
                data-testid="user-name"
                prop:value=move || name.get()
                on:input=move |ev| name.set(event_target_value(&ev))
            />
            <select
                aria-label="Роль"
                data-testid="user-role"
                prop:value=move || role.get()
                on:change=move |ev| role.set(event_target_value(&ev))
            >
                {ROLES
                    .into_iter()
                    .map(|r| view! { <option value=r selected=move || role.get() == r>{role_label(r)}</option> })
                    .collect_view()}
            </select>
            <input
                type="password"
                required
                placeholder="Начальный пароль"
                aria-label="Начальный пароль"
                autocomplete="new-password"
                data-testid="user-password"
                prop:value=move || password.get()
                on:input=move |ev| password.set(event_target_value(&ev))
            />
            <button type="submit" class="btn btn--primary" data-testid="user-add" disabled=move || create.pending().get()>
                "Добавить"
            </button>
        </form>
    }
}

#[component]
fn UserLine(
    user: UserRow,
    is_me: bool,
    version: RwSignal<u32>,
    error: RwSignal<Option<String>>,
    done: RwSignal<Option<String>>,
) -> impl IntoView {
    let id = StoredValue::new(user.id.clone());
    let blocked = user.status == "blocked";
    let single_role = (user.roles.len() == 1).then(|| user.roles[0].clone());
    let name = RwSignal::new(user.display_name.clone());
    let original_name = user.display_name.clone();
    let reset_open = RwSignal::new(false);
    let new_password = RwSignal::new(String::new());

    let update = Action::new(move |(id, patch, ok): &(String, UserPatch, &'static str)| {
        let (id, patch, ok) = (id.clone(), patch.clone(), *ok);
        // форма пароля остаётся открытой при ошибке, чтобы можно было исправить ввод
        let keep_form = patch.password.is_some();
        async move { (update_user(id, patch).await, ok, keep_form) }
    });
    Effect::new(move |_| {
        if let Some((result, ok, keep_form)) = update.value().get() {
            match result {
                Ok(Reply::Ok(_)) => {
                    error.set(None);
                    done.set(Some(ok.into()));
                    reset_open.set(false);
                    new_password.set(String::new());
                }
                Ok(other) => {
                    done.set(None);
                    error.set(failure_text(&other));
                }
                Err(_) => {
                    done.set(None);
                    error.set(Some("Не удалось сохранить изменения.".into()));
                }
            }
            // список перечитывается и после ошибки: значения возвращаются к серверным
            if !keep_form || done.get_untracked().is_some() {
                version.update(|v| *v += 1);
            }
        }
    });

    let send = move |patch: UserPatch, ok: &'static str| {
        update.dispatch((id.get_value(), patch, ok));
    };

    view! {
        <div class="qrow" data-testid="user-row" data-email=user.email.clone()>
            <div>
                <input
                    type="text"
                    aria-label="Имя"
                    placeholder="Имя"
                    data-testid="user-name-input"
                    prop:value=move || name.get()
                    on:input=move |ev| name.set(event_target_value(&ev))
                    on:change=move |_| {
                        if name.get_untracked().trim() != original_name.trim() {
                            send(UserPatch::name(&name.get_untracked()), "Имя сохранено.");
                        }
                    }
                />
                <div class="muted">{user.email.clone()} {is_me.then_some(" (вы)")}</div>
            </div>
            <select
                aria-label="Роль"
                data-testid="user-role-select"
                on:change=move |ev| send(UserPatch::role(&event_target_value(&ev)), "Роль изменена.")
            >
                {single_role.is_none().then(|| view! { <option value="" selected disabled>"Несколько ролей"</option> })}
                {ROLES
                    .into_iter()
                    .map(|r| {
                        let selected = single_role.as_deref() == Some(r);
                        view! { <option value=r selected=selected>{role_label(r)}</option> }
                    })
                    .collect_view()}
            </select>
            <span class="muted" data-testid="user-status">{if blocked { "Заблокирован" } else { "Активен" }}</span>
            <button
                type="button"
                class="btn"
                data-testid="user-toggle"
                on:click=move |_| {
                    if blocked {
                        send(UserPatch::status("active"), "Пользователь разблокирован.");
                    } else {
                        send(UserPatch::status("blocked"), "Пользователь заблокирован.");
                    }
                }
            >
                {if blocked { "Разблокировать" } else { "Заблокировать" }}
            </button>
            <button type="button" class="btn btn--ghost" data-testid="user-reset" on:click=move |_| reset_open.update(|o| *o = !*o)>
                "Сбросить пароль"
            </button>
            <Show when=move || reset_open.get()>
                <form
                    class="toolbar"
                    method="post"
                    onsubmit="return false"
                    on:submit=move |ev| {
                        ev.prevent_default();
                        send(UserPatch::password(&new_password.get()), "Пароль изменён.");
                    }
                >
                    <input
                        type="password"
                        required
                        placeholder="Новый пароль"
                        aria-label="Новый пароль"
                        autocomplete="new-password"
                        data-testid="password-input"
                        prop:value=move || new_password.get()
                        on:input=move |ev| new_password.set(event_target_value(&ev))
                    />
                    <button type="submit" class="btn btn--primary" data-testid="password-save">"Сохранить"</button>
                </form>
            </Show>
        </div>
    }
}
