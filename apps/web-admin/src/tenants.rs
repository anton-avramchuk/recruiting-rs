//! Экран «Организации»: список на гриде, создание, приостановка и активация.
use std::sync::Arc;

use admin_contracts::TenantDto;
use grid_contracts::{GridPage, GridQuery, Operator};
use leptos::prelude::*;
use ui_data::{EntityGrid, Mutation, MutationConfig};
use ui_kit::{
    FormFields, Notice, NoticeKind, PageHeader, Reply, StatusBadge, api::Method, classify,
    form_fields, grid::Column, typed,
};

/// Страница организаций по запросу грида.
pub async fn list_tenants_grid(
    query: GridQuery,
) -> Result<Reply<GridPage<TenantDto>>, ServerFnError> {
    let path = ui_kit::grid::grid_path("/api/tenants", &query);
    let reply = ui_kit::api::call(Method::Get, &path, None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn create_tenant(
    slug: String,
    name: String,
    owner_email: String,
    owner_password: String,
    owner_name: String,
) -> Result<Reply<TenantDto>, ServerFnError> {
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
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn set_tenant_status(id: String, action: String) -> Result<Reply<()>, ServerFnError> {
    if action != "suspend" && action != "activate" {
        return Err(ServerFnError::new("unknown action"));
    }
    if id.parse::<uuid_like::Id>().is_err() {
        return Err(ServerFnError::new("invalid id"));
    }
    let reply =
        ui_kit::api::call(Method::Post, &format!("/api/tenants/{id}/{action}"), None).await?;
    Ok(classify(reply.status, &reply.body, typed::<TenantDto>).map(|_| ()))
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

    async fn run(self) -> Result<Reply<TenantDto>, ServerFnError> {
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

fn status_options() -> Signal<Vec<(String, String)>> {
    Signal::stored(vec![
        ("active".to_string(), "Активные".to_string()),
        ("suspended".to_string(), "Приостановленные".to_string()),
        ("provisioning".to_string(), "Создаются".to_string()),
    ])
}

fn tenant_columns() -> Vec<Column<TenantDto>> {
    vec![
        Column::new("slug", "Адрес", |t: &TenantDto| {
            view! { <span data-slug=t.slug.clone()>{t.slug.clone()}</span> }.into_any()
        })
        .sortable()
        .filter_text(),
        Column::new("name", "Название", |t: &TenantDto| {
            view! { <span>{t.name.clone()}</span> }.into_any()
        })
        .sortable()
        .filter_text(),
        Column::new("status", "Статус", |t: &TenantDto| {
            view! { <StatusBadge status=t.status.clone() /> }.into_any()
        })
        .sortable()
        .filter_select(status_options(), Operator::Eq),
    ]
}

#[component]
pub fn TenantsPage() -> impl IntoView {
    // перечитывание списка после создания и смены состояния
    let refresh = RwSignal::new(0u32);
    let toggle_message = RwSignal::new(None::<String>);
    let create_message = RwSignal::new(None::<String>);
    let created = RwSignal::new(false);

    let toggle = Mutation::new(
        |(id, action): (String, String)| set_tenant_status(id, action),
        move |()| refresh.update(|v| *v += 1),
        MutationConfig {
            fallback: "Не удалось изменить состояние. Попробуйте позже.",
            conflict: |_| "Состояние организации уже изменилось, список обновлён.".into(),
            message: Some(toggle_message),
        },
    );

    let create_form = NodeRef::<leptos::html::Form>::new();
    let create = Mutation::new(
        |input: CreateTenant| input.run(),
        move |_| {
            created.set(true);
            refresh.update(|v| *v += 1);
            if let Some(form) = create_form.get_untracked() {
                form.reset();
            }
        },
        MutationConfig {
            fallback: "Не удалось создать организацию. Попробуйте позже.",
            conflict: |_| "Такой адрес уже занят.".into(),
            message: Some(create_message),
        },
    );

    let extra: ui_kit::grid::RowExtra<TenantDto> = Arc::new(move |t: &TenantDto| {
        let (action, label) = if t.status == "suspended" {
            ("activate", "Активировать")
        } else {
            ("suspend", "Приостановить")
        };
        if t.status == "provisioning" {
            return ().into_any();
        }
        let id = t.id.to_string();
        view! {
            <button
                type="button"
                class="btn"
                data-testid="tenant-toggle"
                on:click=move |_| toggle.run((id.clone(), action.to_string()))
            >
                {label}
            </button>
        }
        .into_any()
    });

    view! {
        <PageHeader title="Организации" />

        {move || {
            toggle_message
                .get()
                .map(|m| view! { <Notice kind=NoticeKind::Error>{m}</Notice> })
        }}

        <EntityGrid
            columns=tenant_columns()
            load=list_tenants_grid
            search_fields=vec!["slug", "name"]
            search_placeholder="Поиск по названию или адресу"
            empty_title="Организаций не найдено"
            empty_hint="Измените фильтр или создайте организацию ниже."
            extra_actions=Some(extra)
            refresh=Some(Signal::from(refresh))
        />

        <h2 style="margin-top: 24px">"Новая организация"</h2>
        <form
            node_ref=create_form
            class="card"
            method="post"
            onsubmit="return false"
            on:submit=move |ev| {
                ev.prevent_default();
                created.set(false);
                create.run(CreateTenant::from_fields(&form_fields(&ev)));
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
                create_message
                    .get()
                    .map(|m| view! { <Notice kind=NoticeKind::Error>{m}</Notice> })
            }}
            {move || {
                created
                    .get()
                    .then(|| view! { <Notice kind=NoticeKind::Info>"Организация создана."</Notice> })
            }}
            <p>
                <button type="submit" class="btn btn--primary" disabled=move || create.pending().get()>
                    "Создать организацию"
                </button>
            </p>
        </form>
    }
}
