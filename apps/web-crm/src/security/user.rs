//! Страницы пользователя: создание и правка (профиль, роль, доступ, пароль).
use leptos::prelude::*;
use leptos_router::{
    components::A,
    hooks::{use_navigate, use_params_map},
};
use ui_kit::{EmptyState, Notice, NoticeKind};

use super::{
    role_label,
    server::{NewUserForm, UserPatch, UserRow, create_user, failure_text, list_users, update_user},
    view::{SecurityShell, roles_label},
};
use crate::funnel::server::Reply;

const ROLES: [&str; 3] = ["owner", "recruiter", "viewer"];

fn back_link() -> ViewFn {
    ViewFn::from(
        || view! { <A href="/security" attr:class="btn" attr:data-testid="back-to-users">"К пользователям"</A> },
    )
}

#[component]
pub fn NewUserPage() -> impl IntoView {
    let navigate = use_navigate();
    let email = RwSignal::new(String::new());
    let name = RwSignal::new(String::new());
    let role = RwSignal::new("recruiter".to_string());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let create = Action::new(move |form: &NewUserForm| {
        let form = form.clone();
        async move { create_user(form).await }
    });
    Effect::new(move |_| {
        if let Some(result) = create.value().get() {
            match result {
                Ok(Reply::Ok(_)) => navigate("/security", Default::default()),
                Ok(other) => error.set(failure_text(&other)),
                Err(_) => error.set(Some("Не удалось добавить пользователя.".into())),
            }
        }
    });
    view! {
        <SecurityShell title="Новый пользователь" actions=back_link()>
            {move || error.get().map(|m| view! { <div data-testid="users-error"><Notice kind=NoticeKind::Error>{m}</Notice></div> })}
            <form
                class="card section"
                method="post"
                onsubmit="return false"
                on:submit=move |ev| {
                    ev.prevent_default();
                    error.set(None);
                    create
                        .dispatch(NewUserForm {
                            email: email.get(),
                            display_name: name.get(),
                            role: role.get(),
                            password: password.get(),
                        });
                }
            >
                <div class="form-grid">
                    <label>
                        "Email"
                        <input
                            type="email"
                            required
                            data-testid="user-email"
                            prop:value=move || email.get()
                            on:input=move |ev| email.set(event_target_value(&ev))
                        />
                    </label>
                    <label>
                        "Имя"
                        <input
                            type="text"
                            data-testid="user-name"
                            prop:value=move || name.get()
                            on:input=move |ev| name.set(event_target_value(&ev))
                        />
                    </label>
                    <label>
                        "Роль"
                        <select data-testid="user-role" prop:value=move || role.get() on:change=move |ev| role.set(event_target_value(&ev))>
                            {ROLES
                                .into_iter()
                                .map(|r| view! { <option value=r selected=move || role.get() == r>{role_label(r)}</option> })
                                .collect_view()}
                        </select>
                    </label>
                    <label>
                        "Начальный пароль"
                        <input
                            type="password"
                            required
                            autocomplete="new-password"
                            data-testid="user-password"
                            prop:value=move || password.get()
                            on:input=move |ev| password.set(event_target_value(&ev))
                        />
                    </label>
                </div>
                <p>
                    <button type="submit" class="btn btn--primary" data-testid="user-add" disabled=move || create.pending().get()>
                        "Создать"
                    </button>
                </p>
            </form>
        </SecurityShell>
    }
}

#[component]
pub fn UserPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.read().get("id").unwrap_or_default();
    let version = RwSignal::new(0u32);
    let users = Resource::new(move || version.get(), |_| list_users());
    view! {
        <SecurityShell title="Пользователь" actions=back_link()>
            <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
                {move || {
                    users
                        .get()
                        .map(|result| match result {
                            Ok(Reply::Ok(list)) => {
                                match list.into_iter().find(|u| u.id == id()) {
                                    Some(user) => view! { <UserEditor user version /> }.into_any(),
                                    None => {
                                        view! { <EmptyState title="Пользователь не найден" hint="Возможно, он из другой организации." /> }
                                            .into_any()
                                    }
                                }
                            }
                            _ => {
                                view! { <Notice kind=NoticeKind::Error>"Не удалось загрузить пользователя. Попробуйте позже."</Notice> }
                                    .into_any()
                            }
                        })
                }}
            </Transition>
        </SecurityShell>
    }
}

/// Одно изменение пользователя: что отправить и что сказать при успехе.
type Change = (UserPatch, &'static str);

#[component]
fn UserEditor(user: UserRow, version: RwSignal<u32>) -> impl IntoView {
    let id = StoredValue::new(user.id.clone());
    let blocked = user.status == "blocked";
    let single_role = (user.roles.len() == 1).then(|| user.roles[0].clone());
    let name = RwSignal::new(user.display_name.clone());
    let role = RwSignal::new(single_role.clone().unwrap_or_default());
    let new_password = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let done = RwSignal::new(None::<String>);

    let update = Action::new(move |change: &Change| {
        let (patch, ok) = change.clone();
        // форма пароля и имени сохраняет введённое при ошибке
        let keep_input = patch.password.is_some() || patch.display_name.is_some();
        let id = id.get_value();
        async move { (update_user(id, patch).await, ok, keep_input) }
    });
    Effect::new(move |_| {
        if let Some((result, ok, keep_input)) = update.value().get() {
            match result {
                Ok(Reply::Ok(_)) => {
                    error.set(None);
                    done.set(Some(ok.into()));
                    new_password.set(String::new());
                    version.update(|v| *v += 1);
                }
                Ok(other) => {
                    done.set(None);
                    error.set(failure_text(&other));
                    if !keep_input {
                        version.update(|v| *v += 1);
                    }
                }
                Err(_) => {
                    done.set(None);
                    error.set(Some("Не удалось сохранить изменения.".into()));
                }
            }
        }
    });
    let send = move |patch: UserPatch, ok: &'static str| {
        error.set(None);
        done.set(None);
        update.dispatch((patch, ok));
    };

    view! {
        {move || error.get().map(|m| view! { <div data-testid="users-error"><Notice kind=NoticeKind::Error>{m}</Notice></div> })}
        {move || done.get().map(|m| view! { <div data-testid="users-ok"><Notice kind=NoticeKind::Info>{m}</Notice></div> })}
        <p class="muted" data-testid="user-heading">{user.email.clone()} " · " {roles_label(&user)}</p>

        <form
            class="card section"
            method="post"
            onsubmit="return false"
            on:submit=move |ev| {
                ev.prevent_default();
                send(UserPatch::name(&name.get()), "Имя сохранено.");
            }
        >
            <h2 class="sec-h">"Профиль"</h2>
            <div class="form-grid">
                <label>
                    "Email"
                    <input type="email" disabled value=user.email.clone() data-testid="user-email-view" />
                </label>
                <label>
                    "Имя"
                    <input
                        type="text"
                        data-testid="user-name-input"
                        prop:value=move || name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                </label>
            </div>
            <p><button type="submit" class="btn btn--primary" data-testid="user-name-save">"Сохранить"</button></p>
        </form>

        <form
            class="card section"
            method="post"
            onsubmit="return false"
            on:submit=move |ev| {
                ev.prevent_default();
                send(UserPatch::role(&role.get()), "Роль изменена.");
            }
        >
            <h2 class="sec-h">"Роль"</h2>
            <div class="form-grid">
                <label>
                    "Роль"
                    <select data-testid="user-role-select" prop:value=move || role.get() on:change=move |ev| role.set(event_target_value(&ev))>
                        {single_role.is_none().then(|| view! { <option value="" selected disabled>"Несколько ролей"</option> })}
                        {ROLES
                            .into_iter()
                            .map(|r| view! { <option value=r selected=move || role.get() == r>{role_label(r)}</option> })
                            .collect_view()}
                    </select>
                </label>
            </div>
            <p>
                <button type="submit" class="btn btn--primary" data-testid="user-role-save" disabled=move || role.get().is_empty()>
                    "Сохранить роль"
                </button>
            </p>
        </form>

        <div class="card section">
            <h2 class="sec-h">"Доступ"</h2>
            <p>
                "Статус: "
                <span class=if blocked { "badge badge--off" } else { "badge" } data-testid="user-status">
                    {if blocked { "Заблокирован" } else { "Активен" }}
                </span>
            </p>
            <p class="muted">"Заблокированный пользователь не может войти, его сессии завершаются."</p>
            <p>
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
            </p>
        </div>

        <form
            class="card section"
            method="post"
            onsubmit="return false"
            on:submit=move |ev| {
                ev.prevent_default();
                send(UserPatch::password(&new_password.get()), "Пароль изменён.");
            }
        >
            <h2 class="sec-h">"Пароль"</h2>
            <div class="form-grid">
                <label>
                    "Новый пароль"
                    <input
                        type="password"
                        required
                        autocomplete="new-password"
                        data-testid="password-input"
                        prop:value=move || new_password.get()
                        on:input=move |ev| new_password.set(event_target_value(&ev))
                    />
                </label>
            </div>
            <p class="muted">"Все сессии пользователя будут завершены."</p>
            <p><button type="submit" class="btn btn--primary" data-testid="password-save">"Сменить пароль"</button></p>
        </form>
    }
}
