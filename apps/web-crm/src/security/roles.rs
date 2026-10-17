//! Вкладка «Роли»: встроенные и собственные роли организации.
use leptos::prelude::*;
use leptos_router::{
    components::A,
    hooks::{use_navigate, use_params_map},
};
use ui_kit::{EmptyState, Notice, NoticeKind};

use super::{
    permission_label,
    server::{RoleRow, create_role, delete_role, list_roles, role_failure_text, update_role},
    view::SecurityShell,
};
use crate::funnel::server::Reply;

fn back_link() -> ViewFn {
    ViewFn::from(
        || view! { <A href="/security/roles" attr:class="btn" attr:data-testid="back-to-roles">"К ролям"</A> },
    )
}

#[component]
pub fn RolesPage() -> impl IntoView {
    let roles = Resource::new(|| (), |_| list_roles());
    view! {
        <SecurityShell
            title="Безопасность"
            actions=ViewFn::from(|| {
                view! {
                    <A href="/security/roles/new" attr:class="btn btn--primary" attr:data-testid="role-new">
                        "Добавить роль"
                    </A>
                }
            })
        >
            <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
                {move || {
                    roles
                        .get()
                        .map(|result| match result {
                            Ok(Reply::Ok(list)) => {
                                view! {
                                    <div class="card list-card" data-testid="roles">
                                        {list.into_iter().map(|r| view! { <RoleLine role=r /> }).collect_view()}
                                    </div>
                                }
                                    .into_any()
                            }
                            Ok(Reply::Forbidden) => {
                                view! { <EmptyState title="Нет доступа" hint="Управлять ролями может только владелец организации." /> }
                                    .into_any()
                            }
                            _ => {
                                view! { <Notice kind=NoticeKind::Error>"Не удалось загрузить роли. Попробуйте позже."</Notice> }
                                    .into_any()
                            }
                        })
                }}
            </Transition>
        </SecurityShell>
    }
}

#[component]
fn RoleLine(role: RoleRow) -> impl IntoView {
    let custom = role.kind == "custom";
    let chips = role
        .permissions
        .iter()
        .map(|p| view! { <span class="chip">{permission_label(p)}</span> })
        .collect_view();
    let users = format!("Пользователей: {}", role.users);
    let name = role.name.clone();
    let body = view! {
        <span class="qrow__main">
            <span class="nm" data-testid="role-name">{name}</span>
            <span class="sub">{if custom { "Своя роль" } else { "Встроенная роль" }}</span>
        </span>
        <span class="chips" data-testid="role-permissions">{chips}</span>
        <span class="muted" data-testid="role-users">{users}</span>
    };
    if custom {
        view! {
            <A href=format!("/security/roles/{}", role.id) attr:class="qrow" attr:data-testid="role-row" attr:data-kind="custom">
                {body}
            </A>
        }
        .into_any()
    } else {
        view! {
            <div class="qrow" data-testid="role-row" data-kind="builtin">{body}</div>
        }
        .into_any()
    }
}

/// Флажки прав с зависимостями: запись и удаление включают чтение, снятие чтения снимает остальное.
#[component]
fn PermissionBoxes(
    read: RwSignal<bool>,
    write: RwSignal<bool>,
    delete: RwSignal<bool>,
) -> impl IntoView {
    view! {
        <div class="form-grid">
            <label class="check">
                <input
                    type="checkbox"
                    data-testid="perm-read"
                    prop:checked=move || read.get()
                    on:change=move |ev| {
                        let on = event_target_checked(&ev);
                        read.set(on);
                        if !on {
                            write.set(false);
                            delete.set(false);
                        }
                    }
                />
                "Чтение данных"
            </label>
            <label class="check">
                <input
                    type="checkbox"
                    data-testid="perm-write"
                    prop:checked=move || write.get()
                    on:change=move |ev| {
                        let on = event_target_checked(&ev);
                        write.set(on);
                        if on {
                            read.set(true);
                        } else {
                            delete.set(false);
                        }
                    }
                />
                "Запись данных"
            </label>
            <label class="check">
                <input
                    type="checkbox"
                    data-testid="perm-delete"
                    prop:checked=move || delete.get()
                    on:change=move |ev| {
                        let on = event_target_checked(&ev);
                        delete.set(on);
                        if on {
                            read.set(true);
                            write.set(true);
                        }
                    }
                />
                "Удаление данных"
            </label>
        </div>
    }
}

fn selected_permissions(read: bool, write: bool, delete: bool) -> Vec<String> {
    [
        (read, "read_data"),
        (write, "write_data"),
        (delete, "delete_data"),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, name)| name.to_string())
    .collect()
}

#[component]
pub fn NewRolePage() -> impl IntoView {
    let navigate = use_navigate();
    let name = RwSignal::new(String::new());
    let (read, write, delete) = (
        RwSignal::new(true),
        RwSignal::new(false),
        RwSignal::new(false),
    );
    let error = RwSignal::new(None::<String>);
    let create = Action::new(move |(name, perms): &(String, Vec<String>)| {
        let (name, perms) = (name.clone(), perms.clone());
        async move { create_role(name, perms).await }
    });
    Effect::new(move |_| {
        if let Some(result) = create.value().get() {
            match result {
                Ok(Reply::Ok(_)) => navigate("/security/roles", Default::default()),
                Ok(other) => error.set(role_failure_text(&other)),
                Err(_) => error.set(Some("Не удалось создать роль.".into())),
            }
        }
    });
    view! {
        <SecurityShell title="Новая роль" actions=back_link()>
            {move || error.get().map(|m| view! { <div data-testid="roles-error"><Notice kind=NoticeKind::Error>{m}</Notice></div> })}
            <form
                class="card section"
                method="post"
                onsubmit="return false"
                on:submit=move |ev| {
                    ev.prevent_default();
                    error.set(None);
                    create.dispatch((name.get(), selected_permissions(read.get(), write.get(), delete.get())));
                }
            >
                <div class="form-grid">
                    <label>
                        "Название"
                        <input
                            type="text"
                            required
                            maxlength="60"
                            data-testid="role-name-input"
                            prop:value=move || name.get()
                            on:input=move |ev| name.set(event_target_value(&ev))
                        />
                    </label>
                </div>
                <h2 class="sec-h">"Права"</h2>
                <PermissionBoxes read write delete />
                <p class="muted">"Управлять пользователями и ролями может только владелец."</p>
                <p>
                    <button type="submit" class="btn btn--primary" data-testid="role-save" disabled=move || create.pending().get()>
                        "Создать"
                    </button>
                </p>
            </form>
        </SecurityShell>
    }
}

#[component]
pub fn RolePage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.read().get("id").unwrap_or_default();
    let version = RwSignal::new(0u32);
    let roles = Resource::new(move || version.get(), |_| list_roles());
    let error = RwSignal::new(None::<String>);
    let done = RwSignal::new(None::<String>);
    view! {
        <SecurityShell title="Роль" actions=back_link()>
            {move || error.get().map(|m| view! { <div data-testid="roles-error"><Notice kind=NoticeKind::Error>{m}</Notice></div> })}
            {move || done.get().map(|m| view! { <div data-testid="roles-ok"><Notice kind=NoticeKind::Info>{m}</Notice></div> })}
            <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
                {move || {
                    roles
                        .get()
                        .map(|result| match result {
                            Ok(Reply::Ok(list)) => {
                                match list.into_iter().find(|r| r.id == id() && r.kind == "custom") {
                                    Some(role) => view! { <RoleEditor role version error done /> }.into_any(),
                                    None => {
                                        view! { <EmptyState title="Роль не найдена" hint="Встроенные роли изменить нельзя." /> }
                                            .into_any()
                                    }
                                }
                            }
                            _ => {
                                view! { <Notice kind=NoticeKind::Error>"Не удалось загрузить роль. Попробуйте позже."</Notice> }
                                    .into_any()
                            }
                        })
                }}
            </Transition>
        </SecurityShell>
    }
}

#[component]
fn RoleEditor(
    role: RoleRow,
    version: RwSignal<u32>,
    error: RwSignal<Option<String>>,
    done: RwSignal<Option<String>>,
) -> impl IntoView {
    let navigate = use_navigate();
    let id = StoredValue::new(role.id.clone());
    let has = |p: &str| role.permissions.iter().any(|x| x == p);
    let (read, write, delete) = (
        RwSignal::new(has("read_data")),
        RwSignal::new(has("write_data")),
        RwSignal::new(has("delete_data")),
    );
    let name = RwSignal::new(role.name.clone());
    let users = role.users;

    let save = Action::new(move |(name, perms): &(String, Vec<String>)| {
        let (name, perms, id) = (name.clone(), perms.clone(), id.get_value());
        async move { update_role(id, name, perms).await }
    });
    Effect::new(move |_| {
        if let Some(result) = save.value().get() {
            match result {
                Ok(Reply::Ok(_)) => {
                    error.set(None);
                    done.set(Some("Роль сохранена.".into()));
                    version.update(|v| *v += 1);
                }
                Ok(other) => {
                    done.set(None);
                    error.set(role_failure_text(&other));
                }
                Err(_) => {
                    done.set(None);
                    error.set(Some("Не удалось сохранить роль.".into()));
                }
            }
        }
    });

    let remove = Action::new(move |_: &()| {
        let id = id.get_value();
        async move { delete_role(id).await }
    });
    Effect::new(move |_| {
        if let Some(result) = remove.value().get() {
            match result {
                Ok(Reply::Ok(())) => navigate("/security/roles", Default::default()),
                Ok(other) => {
                    done.set(None);
                    error.set(role_failure_text(&other));
                }
                Err(_) => error.set(Some("Не удалось удалить роль.".into())),
            }
        }
    });

    view! {
        <p class="muted" data-testid="role-heading">{format!("Пользователей с ролью: {users}")}</p>
        <form
            class="card section"
            method="post"
            onsubmit="return false"
            on:submit=move |ev| {
                ev.prevent_default();
                error.set(None);
                done.set(None);
                save.dispatch((name.get(), selected_permissions(read.get(), write.get(), delete.get())));
            }
        >
            <div class="form-grid">
                <label>
                    "Название"
                    <input
                        type="text"
                        required
                        maxlength="60"
                        data-testid="role-name-input"
                        prop:value=move || name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                </label>
            </div>
            <h2 class="sec-h">"Права"</h2>
            <PermissionBoxes read write delete />
            <p class="muted">"Изменение прав завершает сессии пользователей с этой ролью."</p>
            <p>
                <button type="submit" class="btn btn--primary" data-testid="role-save" disabled=move || save.pending().get()>
                    "Сохранить"
                </button>
            </p>
        </form>
        <div class="card section">
            <h2 class="sec-h">"Удаление"</h2>
            <p class="muted">"Роль можно удалить, только если она никому не назначена."</p>
            <p>
                <button
                    type="button"
                    class="btn"
                    data-testid="role-delete"
                    on:click=move |_| {
                        error.set(None);
                        done.set(None);
                        remove.dispatch(());
                    }
                >
                    "Удалить роль"
                </button>
            </p>
        </div>
    }
}
