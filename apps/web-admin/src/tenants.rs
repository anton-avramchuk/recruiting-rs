//! Экран «Организации»: список, создание, приостановка и активация.
use admin_contracts::TenantPage;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use ui_kit::{
    EmptyState, FormFields, Notice, NoticeKind, PageHeader, StatusBadge, api::Method, form_fields,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreateOutcome {
    Created,
    SlugTaken,
    Invalid(String),
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusOutcome {
    Done,
    /// Состояние уже изменилось (например, организация уже приостановлена).
    Conflict,
    Unavailable,
}

pub async fn list_tenants(
    status: String,
    q: String,
    page: i64,
) -> Result<Option<TenantPage>, ServerFnError> {
    let mut path = format!("/api/tenants?page={}", page.max(1));
    if !status.is_empty() {
        path.push_str(&format!("&status={status}"));
    }
    if !q.trim().is_empty() {
        path.push_str(&format!("&q={}", ui_kit::encode_query_value(q.trim())));
    }
    let reply = ui_kit::api::call(Method::Get, &path, None).await?;
    if reply.status != 200 {
        return Ok(None);
    }
    Ok(serde_json::from_value(reply.body).ok())
}

pub async fn create_tenant(
    slug: String,
    name: String,
    owner_email: String,
    owner_password: String,
    owner_name: String,
) -> Result<CreateOutcome, ServerFnError> {
    let body = serde_json::json!({
        "slug": slug.trim(),
        "name": name.trim(),
        "owner": {
            "email": owner_email.trim(),
            "password": owner_password,
            "display_name": owner_name.trim(),
        },
    });
    let reply = ui_kit::api::call(Method::Post, "/api/tenants", Some(body)).await?;
    Ok(match reply.status {
        201 => CreateOutcome::Created,
        409 => CreateOutcome::SlugTaken,
        400 => CreateOutcome::Invalid(
            reply.body["detail"]
                .as_str()
                .unwrap_or("Проверьте введённые данные")
                .to_string(),
        ),
        _ => CreateOutcome::Unavailable,
    })
}

pub async fn set_tenant_status(id: String, action: String) -> Result<StatusOutcome, ServerFnError> {
    if action != "suspend" && action != "activate" {
        return Err(ServerFnError::new("unknown action"));
    }
    if id.parse::<uuid_like::Id>().is_err() {
        return Err(ServerFnError::new("invalid id"));
    }
    let reply =
        ui_kit::api::call(Method::Post, &format!("/api/tenants/{id}/{action}"), None).await?;
    Ok(match reply.status {
        200 => StatusOutcome::Done,
        409 | 404 => StatusOutcome::Conflict,
        _ => StatusOutcome::Unavailable,
    })
}

/// Значение поля формы или пустая строка.
fn field(fields: &FormFields, name: &str) -> String {
    fields.get(name).cloned().unwrap_or_default()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreateTenant {
    pub slug: String,
    pub name: String,
    pub owner_email: String,
    pub owner_password: String,
    pub owner_name: String,
}

impl CreateTenant {
    fn from_fields(f: &FormFields) -> Self {
        Self {
            slug: field(f, "slug"),
            name: field(f, "name"),
            owner_email: field(f, "owner_email"),
            owner_password: field(f, "owner_password"),
            owner_name: field(f, "owner_name"),
        }
    }

    async fn run(self) -> Result<CreateOutcome, ServerFnError> {
        create_tenant(
            self.slug,
            self.name,
            self.owner_email,
            self.owner_password,
            self.owner_name,
        )
        .await
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetTenantStatus {
    pub id: String,
    pub action: String,
}

impl SetTenantStatus {
    async fn run(self) -> Result<StatusOutcome, ServerFnError> {
        set_tenant_status(self.id, self.action).await
    }
}

/// Минимальная проверка формата UUID без лишней зависимости.
mod uuid_like {
    use std::str::FromStr;

    pub struct Id;

    impl FromStr for Id {
        type Err = ();

        fn from_str(s: &str) -> Result<Self, ()> {
            let ok = s.len() == 36
                && s.bytes().enumerate().all(|(i, b)| match i {
                    8 | 13 | 18 | 23 => b == b'-',
                    _ => b.is_ascii_hexdigit(),
                });
            if ok { Ok(Id) } else { Err(()) }
        }
    }
}

#[component]
pub fn TenantsPage() -> impl IntoView {
    let (status, set_status) = signal(String::new());
    let (query, set_query) = signal(String::new());
    let (page, set_page) = signal(1i64);

    let create = Action::new(|input: &CreateTenant| input.clone().run());
    let toggle = Action::new(|input: &SetTenantStatus| input.clone().run());

    let tenants = Resource::new(
        move || {
            (
                status.get(),
                query.get(),
                page.get(),
                create.version().get(),
                toggle.version().get(),
            )
        },
        |(status, query, page, _, _)| list_tenants(status, query, page),
    );

    // после успешного создания форма очищается
    let create_form = NodeRef::<leptos::html::Form>::new();
    Effect::new(move |_| {
        if let Some(Ok(CreateOutcome::Created)) = create.value().get()
            && let Some(form) = create_form.get()
        {
            form.reset();
        }
    });

    let create_message = move || match create.value().get() {
        Some(Ok(CreateOutcome::Created)) => {
            Some((NoticeKind::Info, "Организация создана.".to_string()))
        }
        Some(Ok(CreateOutcome::SlugTaken)) => {
            Some((NoticeKind::Error, "Такой адрес уже занят.".to_string()))
        }
        Some(Ok(CreateOutcome::Invalid(m))) => Some((NoticeKind::Error, m)),
        Some(Ok(CreateOutcome::Unavailable)) | Some(Err(_)) => Some((
            NoticeKind::Error,
            "Не удалось создать организацию. Попробуйте позже.".to_string(),
        )),
        None => None,
    };
    let toggle_message = move || match toggle.value().get() {
        Some(Ok(StatusOutcome::Conflict)) => {
            Some("Состояние организации уже изменилось, список обновлён.")
        }
        Some(Ok(StatusOutcome::Unavailable)) | Some(Err(_)) => {
            Some("Не удалось изменить состояние. Попробуйте позже.")
        }
        _ => None,
    };

    view! {
        <PageHeader title="Организации" />

        <div class="toolbar">
            <input
                type="search"
                placeholder="Поиск по названию или адресу"
                aria-label="Поиск"
                data-testid="search"
                on:input=move |ev| {
                    set_query.set(event_target_value(&ev));
                    set_page.set(1);
                }
            />
            <select
                aria-label="Статус"
                data-testid="status-filter"
                on:change=move |ev| {
                    set_status.set(event_target_value(&ev));
                    set_page.set(1);
                }
            >
                <option value="">"Все статусы"</option>
                <option value="active">"Активные"</option>
                <option value="suspended">"Приостановленные"</option>
            </select>
        </div>

        {move || {
            toggle_message()
                .map(|m| view! { <Notice kind=NoticeKind::Error>{m}</Notice> })
        }}

        <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
            {move || {
                tenants
                    .get()
                    .map(|result| match result {
                        Ok(Some(data)) if data.items.is_empty() => {
                            view! {
                                <EmptyState
                                    title="Организаций не найдено"
                                    hint="Измените фильтр или создайте организацию ниже."
                                />
                            }
                                .into_any()
                        }
                        Ok(Some(data)) => {
                            let total = data.total;
                            let pages = (data.total + data.page_size - 1) / data.page_size;
                            let at_end = move || page.get() >= pages;
                            view! {
                                <div class="table-wrap">
                                    <table data-testid="tenants">
                                        <thead>
                                            <tr>
                                                <th>"Адрес"</th>
                                                <th>"Название"</th>
                                                <th>"Статус"</th>
                                                <th></th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {data
                                                .items
                                                .into_iter()
                                                .map(|t| {
                                                    let (action, label) = if t.status == "suspended" {
                                                        ("activate", "Активировать")
                                                    } else {
                                                        ("suspend", "Приостановить")
                                                    };
                                                    let can_toggle = t.status != "provisioning";
                                                    view! {
                                                        <tr data-slug=t.slug.clone()>
                                                            <td>{t.slug.clone()}</td>
                                                            <td>{t.name.clone()}</td>
                                                            <td>
                                                                <StatusBadge status=t.status.clone() />
                                                            </td>
                                                            <td>
                                                                <Show when=move || can_toggle>
                                                                    <form
                                                                        method="post"
                                                                        onsubmit="return false"
                                                                        on:submit={
                                                                            let id = t.id.to_string();
                                                                            move |ev: leptos::ev::SubmitEvent| {
                                                                                ev.prevent_default();
                                                                                toggle.dispatch(SetTenantStatus {
                                                                                    id: id.clone(),
                                                                                    action: action.to_string(),
                                                                                });
                                                                            }
                                                                        }
                                                                    >
                                                                        <button type="submit" class="btn">{label}</button>
                                                                    </form>
                                                                </Show>
                                                            </td>
                                                        </tr>
                                                    }
                                                })
                                                .collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                                <p class="muted">
                                    {format!("Всего: {total}. Страница {} из {}", data.page, pages.max(1))}
                                </p>
                                <div class="toolbar">
                                    <button
                                        class="btn"
                                        disabled=move || page.get() <= 1
                                        on:click=move |_| set_page.update(|p| *p -= 1)
                                    >
                                        "Назад"
                                    </button>
                                    <button
                                        class="btn"
                                        disabled=at_end
                                        on:click=move |_| set_page.update(|p| *p += 1)
                                    >
                                        "Вперёд"
                                    </button>
                                </div>
                            }
                                .into_any()
                        }
                        _ => {
                            view! { <Notice kind=NoticeKind::Error>"Не удалось загрузить список организаций."</Notice> }
                                .into_any()
                        }
                    })
            }}
        </Transition>

        <h2 style="margin-top: 24px">"Новая организация"</h2>
        <form
            node_ref=create_form
            class="card"
            method="post"
            onsubmit="return false"
            on:submit=move |ev| {
                ev.prevent_default();
                create.dispatch(CreateTenant::from_fields(&form_fields(&ev)));
            }
        >
            <div class="form-grid">
                <label>
                    "Адрес (slug)"
                    <input name="slug" required placeholder="acme" pattern="[a-z0-9-]+" />
                </label>
                <label>"Название" <input name="name" required placeholder="Acme Inc" /></label>
                <label>
                    "Email владельца" <input type="email" name="owner_email" required />
                </label>
                <label>
                    "Пароль владельца"
                    <input type="password" name="owner_password" required minlength="8" autocomplete="new-password" />
                </label>
                <label>"Имя владельца" <input name="owner_name" /></label>
            </div>
            {move || {
                create_message()
                    .map(|(kind, msg)| view! { <Notice kind=kind>{msg}</Notice> })
            }}
            <p>
                <button type="submit" class="btn btn--primary" disabled=move || create.pending().get()>
                    "Создать организацию"
                </button>
            </p>
        </form>
    }
}
