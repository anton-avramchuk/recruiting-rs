//! Справочник должностей контактных лиц.
use client_contracts::PositionDto;
use leptos::prelude::*;
use leptos_router::components::A;
use ui_kit::Reply;
use ui_kit::{EmptyState, Notice, NoticeKind, PageHeader, SessionCtx};

use super::server::{create_position, delete_position, list_positions, rename_position};
use crate::candidates::list::can_write;

#[component]
pub fn PositionsPage() -> impl IntoView {
    let session = expect_context::<SessionCtx>();
    let writable = Signal::derive(move || can_write(session));
    let version = RwSignal::new(0u32);
    let message = RwSignal::new(None::<String>);
    let name = RwSignal::new(String::new());
    let positions = Resource::new(move || version.get(), |_| list_positions());

    let create = Action::new(move |name: &String| {
        let name = name.clone();
        async move { create_position(name).await }
    });
    Effect::new(move |_| {
        if let Some(result) = create.value().get() {
            match result {
                Ok(Reply::Ok(_)) => {
                    message.set(None);
                    name.set(String::new());
                    version.update(|v| *v += 1);
                }
                Ok(other) => {
                    message.set(other.failure_with(|_| "Такая должность уже есть.".into()))
                }
                Err(_) => message.set(Some("Не удалось добавить должность.".into())),
            }
        }
    });

    view! {
        <PageHeader title="Должности контактов">
            <A href="/clients" attr:class="btn">"К заказчикам"</A>
        </PageHeader>
        {move || message.get().map(|m| view! { <div data-testid="positions-error"><Notice kind=NoticeKind::Error>{m}</Notice></div> })}
        <Show when=move || writable.get()>
            <form
                class="toolbar"
                method="post"
                onsubmit="return false"
                on:submit=move |ev| {
                    ev.prevent_default();
                    create.dispatch(name.get());
                }
            >
                <input
                    type="text"
                    required
                    placeholder="Новая должность"
                    aria-label="Название должности"
                    data-testid="position-input"
                    prop:value=move || name.get()
                    on:input=move |ev| name.set(event_target_value(&ev))
                />
                <button type="submit" class="btn btn--primary" data-testid="position-add" disabled=move || create.pending().get()>
                    "Добавить"
                </button>
            </form>
        </Show>
        <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
            {move || {
                positions
                    .get()
                    .map(|result| match result {
                        Ok(Reply::Ok(list)) if list.is_empty() => {
                            view! { <EmptyState title="Должностей пока нет" hint="Добавьте первую должность выше." /> }
                                .into_any()
                        }
                        Ok(Reply::Ok(list)) => {
                            view! {
                                <div class="card list-card" data-testid="positions">
                                    {list
                                        .into_iter()
                                        .map(|p| view! { <PositionRow position=p version message writable /> })
                                        .collect_view()}
                                </div>
                            }
                                .into_any()
                        }
                        _ => {
                            view! {
                                <Notice kind=NoticeKind::Error>"Не удалось загрузить должности. Попробуйте позже."</Notice>
                            }
                                .into_any()
                        }
                    })
            }}
        </Transition>
    }
}

#[component]
fn PositionRow(
    position: PositionDto,
    version: RwSignal<u32>,
    message: RwSignal<Option<String>>,
    writable: Signal<bool>,
) -> impl IntoView {
    let id = position.id.to_string();
    let name = RwSignal::new(position.name.clone());
    let original = position.name;

    let rename = Action::new(move |(id, name): &(String, String)| {
        let (id, name) = (id.clone(), name.clone());
        async move { rename_position(id, name).await }
    });
    Effect::new(move |_| {
        if let Some(result) = rename.value().get() {
            match result {
                Ok(Reply::Ok(_)) => {
                    message.set(None);
                    version.update(|v| *v += 1);
                }
                Ok(other) => {
                    message.set(other.failure_with(|_| "Такая должность уже есть.".into()))
                }
                Err(_) => message.set(Some("Не удалось переименовать.".into())),
            }
        }
    });
    let remove = Action::new(move |id: &String| {
        let id = id.clone();
        async move { delete_position(id).await }
    });
    Effect::new(move |_| {
        if let Some(result) = remove.value().get() {
            match result {
                Ok(Reply::Ok(())) => {
                    message.set(None);
                    version.update(|v| *v += 1);
                }
                Ok(other) => message.set(other.failure_with(|_| {
                    "Должность назначена контактам: сначала смените её у них.".into()
                })),
                Err(_) => message.set(Some("Не удалось удалить должность.".into())),
            }
        }
    });

    let rename_id = id.clone();
    let delete_id = id;
    view! {
        <div class="qrow" data-testid="position-row">
            <Show
                when=move || writable.get()
                fallback={
                    let original = original.clone();
                    move || view! { <span class="nm" data-testid="position-name">{original.clone()}</span> }
                }
            >
                <input
                    type="text"
                    aria-label="Название должности"
                    data-testid="position-name-input"
                    prop:value=move || name.get()
                    on:input=move |ev| name.set(event_target_value(&ev))
                />
                <button
                    class="btn"
                    data-testid="position-rename"
                    on:click={
                        let rename_id = rename_id.clone();
                        move |_| {
                            rename.dispatch((rename_id.clone(), name.get()));
                        }
                    }
                >
                    "Переименовать"
                </button>
                <button
                    class="btn btn--ghost"
                    data-testid="position-delete"
                    on:click={
                        let delete_id = delete_id.clone();
                        move |_| {
                            remove.dispatch(delete_id.clone());
                        }
                    }
                >
                    "Удалить"
                </button>
            </Show>
        </div>
    }
}
