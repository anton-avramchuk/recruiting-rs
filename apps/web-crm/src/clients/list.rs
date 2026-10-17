//! Список заказчиков: поиск, архивные и создание.
use client_contracts::CreateClientRequest;
use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_navigate};
use ui_kit::Reply;
use ui_kit::{EmptyState, Notice, NoticeKind, PageHeader, SessionCtx};

use super::{
    opt,
    server::{create_client, list_clients},
};
use crate::candidates::list::{DebouncedInput, can_write};

#[component]
pub fn ClientsPage() -> impl IntoView {
    let session = expect_context::<SessionCtx>();
    let writable = Signal::derive(move || can_write(session));
    let (query, set_query) = signal(String::new());
    let (_page, set_page) = signal(1i64);
    let archived = RwSignal::new(false);
    let creating = RwSignal::new(false);
    let results = Resource::new(
        move || (query.get(), archived.get()),
        |(q, archived)| list_clients(q, archived),
    );

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

        <div class="toolbar">
            <DebouncedInput
                applied=set_query
                reset_page=set_page
                placeholder="Поиск по названию"
                label="Поиск"
                testid="clients-search"
            />
            <label class="check">
                <input
                    type="checkbox"
                    data-testid="show-archived"
                    prop:checked=move || archived.get()
                    on:change=move |ev| archived.set(event_target_checked(&ev))
                />
                "С архивными"
            </label>
        </div>

        <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
            {move || {
                results
                    .get()
                    .map(|result| match result {
                        Ok(Reply::Ok(list)) if list.is_empty() => {
                            view! {
                                <EmptyState
                                    title="Заказчиков пока нет"
                                    hint="Добавьте заказчика, и к нему можно будет привязывать вакансии."
                                />
                            }
                                .into_any()
                        }
                        Ok(Reply::Ok(list)) => {
                            view! {
                                <div class="card list-card">
                                    <table data-testid="clients">
                                        <thead>
                                            <tr>
                                                <th>"Заказчик"</th>
                                                <th>"Контактов"</th>
                                                <th>"Примечание"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {list
                                                .into_iter()
                                                .map(|c| {
                                                    view! {
                                                        <tr data-testid="client-row">
                                                            <td>
                                                                <A href=format!("/clients/{}", c.id)>{c.name.clone()}</A>
                                                                {c.archived.then(|| view! { <span class="chip">"архив"</span> })}
                                                            </td>
                                                            <td class="mono">{c.contacts_count}</td>
                                                            <td class="muted">{c.note.clone().unwrap_or_default()}</td>
                                                        </tr>
                                                    }
                                                })
                                                .collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            }
                                .into_any()
                        }
                        _ => {
                            view! {
                                <Notice kind=NoticeKind::Error>"Не удалось загрузить заказчиков. Попробуйте позже."</Notice>
                            }
                                .into_any()
                        }
                    })
            }}
        </Transition>
    }
}

#[component]
fn NewClientForm() -> impl IntoView {
    let name = RwSignal::new(String::new());
    let note = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let navigate = use_navigate();
    let create = Action::new(move |request: &CreateClientRequest| {
        let request = request.clone();
        async move { create_client(request).await }
    });
    Effect::new(move |_| {
        if let Some(result) = create.value().get() {
            match result {
                Ok(Reply::Ok(client)) => {
                    error.set(None);
                    navigate(&format!("/clients/{}", client.id), Default::default());
                }
                Ok(other) => {
                    error.set(other.failure_with(|_| "Заказчик с таким названием уже есть.".into()))
                }
                Err(_) => error.set(Some("Не удалось создать заказчика.".into())),
            }
        }
    });

    view! {
        <form
            class="card section"
            method="post"
            onsubmit="return false"
            data-testid="client-form"
            on:submit=move |ev| {
                ev.prevent_default();
                create
                    .dispatch(CreateClientRequest {
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
