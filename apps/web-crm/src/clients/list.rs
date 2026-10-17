//! Список заказчиков на гриде: поиск, сортировка, фильтры, архив и создание.
use client_contracts::{ClientDto, CreateClientRequest};
use grid_contracts::Operator;
use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_navigate};
use ui_data::{EntityGrid, Mutation, MutationConfig};
use ui_kit::{Notice, NoticeKind, PageHeader, SessionCtx, grid::Column};

use super::{
    opt,
    server::{create_client, list_clients_grid},
};
use crate::candidates::list::can_write;

fn client_href(client: &ClientDto) -> String {
    format!("/clients/{}", client.id)
}

fn client_columns() -> Vec<Column<ClientDto>> {
    vec![
        Column::new("name", "Заказчик", |c: &ClientDto| {
            view! { <span data-testid="client-name-cell">{c.name.clone()}</span> }.into_any()
        })
        .sortable()
        .filter_text(),
        Column::new("contacts_count", "Контактов", |c: &ClientDto| {
            view! { <span class="mono">{c.contacts_count}</span> }.into_any()
        })
        .sortable(),
        Column::new("note", "Примечание", |c: &ClientDto| {
            view! { <span class="muted">{c.note.clone().unwrap_or_default()}</span> }.into_any()
        })
        .filter_text(),
        // по умолчанию список показывает активных: «с архивными» и «только архивные» выбираются явно
        Column::new("archived", "Архив", |c: &ClientDto| {
            c.archived
                .then(
                    || view! { <span class="chip" data-testid="archived-row-chip">"архив"</span> },
                )
                .into_any()
        })
        .filter_select(
            Signal::stored(vec![
                ("all".to_string(), "С архивными".to_string()),
                ("true".to_string(), "Только архивные".to_string()),
            ]),
            Operator::Eq,
        )
        .empty_label("Активные"),
    ]
}

#[component]
pub fn ClientsPage() -> impl IntoView {
    let session = expect_context::<SessionCtx>();
    let writable = Signal::derive(move || can_write(session));
    let creating = RwSignal::new(false);

    view! {
        <PageHeader title="Заказчики">
            <A href="/clients/positions" attr:class="btn" attr:data-testid="positions-link">
                "Должности"
            </A>
            <Show when=move || writable.get()>
                <button class="btn btn--primary" data-testid="new-client" on:click=move |_| creating.update(|c| *c = !*c)>
                    "Новый заказчик"
                </button>
            </Show>
        </PageHeader>

        <Show when=move || creating.get()>
            <NewClientForm />
        </Show>

        <EntityGrid
            columns=client_columns()
            load=list_clients_grid
            row_href=Some(client_href as fn(&ClientDto) -> String)
            search_fields=vec!["name"]
            search_placeholder="Поиск по названию"
            empty_title="Заказчиков пока нет"
            empty_hint="Добавьте заказчика, и к нему можно будет привязывать вакансии."
        />
    }
}

#[component]
fn NewClientForm() -> impl IntoView {
    let name = RwSignal::new(String::new());
    let note = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let navigate = use_navigate();
    // переход делает эффект: навигатор браузера не Send
    let created = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        if let Some(id) = created.get() {
            navigate(&format!("/clients/{id}"), Default::default());
        }
    });
    let create = Mutation::new(
        create_client,
        move |client: ClientDto| created.set(Some(client.id.to_string())),
        MutationConfig {
            fallback: "Не удалось создать заказчика.",
            conflict: |_| "Заказчик с таким названием уже есть.".into(),
            message: Some(error),
        },
    );

    view! {
        <form
            class="card section"
            method="post"
            onsubmit="return false"
            data-testid="client-form"
            on:submit=move |ev| {
                ev.prevent_default();
                create
                    .run(CreateClientRequest {
                        name: name.get(),
                        note: opt(&note.get()),
                    });
            }
        >
            <div class="form-grid">
                <label>
                    "Название"
                    <input type="text" required data-testid="client-name" on:input=move |ev| name.set(event_target_value(&ev)) />
                </label>
                <label>
                    "Примечание"
                    <input type="text" data-testid="client-note" on:input=move |ev| note.set(event_target_value(&ev)) />
                </label>
            </div>
            {move || error.get().map(|e| view! { <Notice kind=NoticeKind::Error>{e}</Notice> })}
            <p>
                <button type="submit" class="btn btn--primary" data-testid="client-submit" disabled=move || create.pending().get()>
                    "Создать заказчика"
                </button>
            </p>
        </form>
    }
}
