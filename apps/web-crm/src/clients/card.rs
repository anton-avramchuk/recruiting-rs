//! Карточка заказчика: данные, архивация и контактные лица.
use client_contracts::{
    ClientDetailDto, ContactDto, CreateContactRequest, PositionDto, UpdateClientRequest,
    UpdateContactRequest,
};
use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_params_map};
use ui_kit::Reply;
use ui_kit::{Chip, Dropdown, EmptyState, Notice, NoticeKind, SessionCtx};
use uuid::Uuid;

use super::{
    opt,
    server::{
        add_contact, delete_contact, get_client, list_positions, update_client, update_contact,
    },
};
use crate::candidates::list::can_write;

#[component]
pub fn ClientCardPage() -> impl IntoView {
    let params = use_params_map();
    let session = expect_context::<SessionCtx>();
    let writable = Signal::derive(move || can_write(session));
    let version = RwSignal::new(0u32);
    let detail = Resource::new(
        move || (params.read().get("id").unwrap_or_default(), version.get()),
        |(id, _)| get_client(id),
    );
    let positions = Resource::new(move || version.get(), |_| list_positions());

    view! {
        <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
            {move || {
                detail
                    .get()
                    .map(|result| match result {
                        Ok(Reply::Ok(d)) => {
                            let list = positions
                                .get()
                                .and_then(Result::ok)
                                .and_then(Reply::ok)
                                .unwrap_or_default();
                            view! { <Card detail=d positions=list version writable /> }.into_any()
                        }
                        Ok(Reply::NotFound) => {
                            view! {
                                <EmptyState title="Заказчик не найден" hint="Возможно, ссылка устарела." />
                                <A href="/clients">"К списку заказчиков"</A>
                            }
                                .into_any()
                        }
                        _ => {
                            view! {
                                <Notice kind=NoticeKind::Error>"Не удалось загрузить заказчика. Попробуйте позже."</Notice>
                            }
                                .into_any()
                        }
                    })
            }}
        </Transition>
    }
}

#[component]
fn Card(
    detail: ClientDetailDto,
    positions: Vec<PositionDto>,
    version: RwSignal<u32>,
    writable: Signal<bool>,
) -> impl IntoView {
    let client = detail.client;
    let id = client.id.to_string();
    let archived = client.archived;
    let name = RwSignal::new(client.name.clone());
    let note = RwSignal::new(client.note.clone().unwrap_or_default());
    let message = RwSignal::new(None::<String>);
    let adding = RwSignal::new(false);
    let editing = RwSignal::new(None::<String>);

    let save = Action::new(move |(id, request): &(String, UpdateClientRequest)| {
        let (id, request) = (id.clone(), request.clone());
        async move { update_client(id, request).await }
    });
    Effect::new(move |_| {
        if let Some(result) = save.value().get() {
            match result {
                Ok(Reply::Ok(_)) => {
                    message.set(None);
                    version.update(|v| *v += 1);
                }
                Ok(other) => message
                    .set(other.failure_with(|_| "Заказчик с таким названием уже есть.".into())),
                Err(_) => message.set(Some("Не удалось сохранить.".into())),
            }
        }
    });

    let remove = Action::new(move |contact_id: &String| {
        let contact_id = contact_id.clone();
        async move { delete_contact(contact_id).await }
    });
    Effect::new(move |_| {
        if let Some(result) = remove.value().get() {
            match result {
                Ok(Reply::Ok(())) => version.update(|v| *v += 1),
                Ok(other) => message.set(other.failure()),
                Err(_) => message.set(Some("Не удалось удалить контакт.".into())),
            }
        }
    });

    let client_id = id.clone();
    let contacts = detail
        .contacts
        .into_iter()
        .map(|c| {
            let contact_id = c.id.to_string();
            let delete_id = contact_id.clone();
            let edit_id = contact_id.clone();
            let show_id = contact_id.clone();
            let form_client = client_id.clone();
            let form_positions = positions.clone();
            let form_contact = c.clone();
            view! {
                <div class="card section" data-testid="contact-row">
                    <Show
                        when={
                            let show_id = show_id.clone();
                            move || editing.get().as_deref() == Some(show_id.as_str())
                        }
                        fallback={
                            let c = c.clone();
                            let edit_id = edit_id.clone();
                            let delete_id = delete_id.clone();
                            move || {
                                let c = c.clone();
                                let edit_id = edit_id.clone();
                                let delete_id = delete_id.clone();
                                view! {
                                    <div class="qrow">
                                        <span class="qrow__main">
                                            <span class="nm" data-testid="contact-name">{c.full_name.clone()}</span>
                                            <span class="sub" data-testid="contact-position">
                                                {c.position.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| "должность не указана".into())}
                                            </span>
                                        </span>
                                        <span class="mono">
                                            {[c.email.clone(), c.phone.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · ")}
                                        </span>
                                        <Show when=move || writable.get()>
                                            <span>
                                                <button
                                                    class="btn btn--ghost"
                                                    data-testid="edit-contact"
                                                    on:click={
                                                        let edit_id = edit_id.clone();
                                                        move |_| editing.set(Some(edit_id.clone()))
                                                    }
                                                >
                                                    "Изменить"
                                                </button>
                                                <button
                                                    class="btn btn--ghost"
                                                    data-testid="delete-contact"
                                                    on:click={
                                                        let delete_id = delete_id.clone();
                                                        move |_| {
                                                            remove.dispatch(delete_id.clone());
                                                        }
                                                    }
                                                >
                                                    "Удалить"
                                                </button>
                                            </span>
                                        </Show>
                                    </div>
                                    {c.note.clone().map(|n| view! { <p class="muted">{n}</p> })}
                                }
                            }
                        }
                    >
                        <ContactForm
                            client_id=form_client.clone()
                            initial=form_contact.clone()
                            positions=form_positions.clone()
                            version
                            on_done=Callback::new(move |_| editing.set(None))
                        />
                    </Show>
                </div>
            }
        })
        .collect_view();

    let header_id = id.clone();
    let comment_target = id.clone();
    let toggle_id = id.clone();
    let save_id = id.clone();
    let add_positions = positions.clone();
    let add_client = id.clone();

    view! {
        <div class="page-header">
            <h1 data-testid="client-title">{client.name.clone()}</h1>
            <div class="page-header__actions">
            <A href="/clients" attr:class="btn">"К списку"</A>
            <A href=format!("/applications?client_id={}", client.id) attr:class="btn" attr:data-testid="client-applications">
                "Отклики заказчика"
            </A>
            {archived.then(|| view! { <Chip testid="archived-chip">"архив"</Chip> })}
            <Show when=move || writable.get()>
                <button
                    class="btn"
                    data-testid="archive-toggle"
                    on:click={
                        let toggle_id = toggle_id.clone();
                        move |_| {
                            save.dispatch((
                                toggle_id.clone(),
                                UpdateClientRequest {
                                    archived: Some(!archived),
                                    ..Default::default()
                                },
                            ));
                        }
                    }
                >
                    {if archived { "Вернуть из архива" } else { "В архив" }}
                </button>
            </Show>
            </div>
        </div>
        <span hidden data-testid="client-id">{header_id}</span>

        {move || message.get().map(|m| view! { <div data-testid="client-message"><Notice kind=NoticeKind::Error>{m}</Notice></div> })}

        <Show when=move || writable.get()>
            <form
                class="card section"
                method="post"
                onsubmit="return false"
                data-testid="client-edit"
                on:submit={
                    let save_id = save_id.clone();
                    move |ev| {
                        ev.prevent_default();
                        save.dispatch((
                            save_id.clone(),
                            UpdateClientRequest {
                                name: Some(name.get()),
                                note: Some(opt(&note.get())),
                                archived: None,
                            },
                        ));
                    }
                }
            >
                <div class="form-grid">
                    <label>
                        "Название"
                        <input type="text" required prop:value=move || name.get() data-testid="client-name" on:input=move |ev| name.set(event_target_value(&ev)) />
                    </label>
                    <label>
                        "Примечание"
                        <input type="text" prop:value=move || note.get() data-testid="client-note" on:input=move |ev| note.set(event_target_value(&ev)) />
                    </label>
                </div>
                <p>
                    <button type="submit" class="btn btn--primary" data-testid="client-save" disabled=move || save.pending().get()>
                        "Сохранить"
                    </button>
                </p>
            </form>
        </Show>
        <Show when=move || !writable.get()>
            <p class="muted">{client.note.clone().unwrap_or_default()}</p>
        </Show>

        <div class="sec-h">
            <h2>"Контактные лица"</h2>
            <Show when=move || writable.get()>
                <button class="btn" data-testid="add-contact" on:click=move |_| adding.update(|a| *a = !*a)>
                    "Добавить контакт"
                </button>
            </Show>
        </div>
        <Show when=move || adding.get()>
            <div class="card section">
                <ContactForm
                    client_id=add_client.clone()
                    positions=add_positions.clone()
                    version
                    on_done=Callback::new(move |_| adding.set(false))
                />
            </div>
        </Show>
        <div data-testid="contacts">{contacts}</div>
        <crate::comments::Comments target_type="client" target_id=Signal::derive(move || comment_target.clone()) />
    }
}

/// Форма контакта: создание (без `initial`) или правка.
#[component]
fn ContactForm(
    client_id: String,
    #[prop(optional)] initial: Option<ContactDto>,
    positions: Vec<PositionDto>,
    version: RwSignal<u32>,
    on_done: Callback<()>,
) -> impl IntoView {
    let editing_id = initial.as_ref().map(|c| c.id.to_string());
    let full_name = RwSignal::new(
        initial
            .as_ref()
            .map(|c| c.full_name.clone())
            .unwrap_or_default(),
    );
    let position = RwSignal::new(
        initial
            .as_ref()
            .and_then(|c| c.position.as_ref().map(|p| p.id.to_string()))
            .unwrap_or_default(),
    );
    let email = RwSignal::new(
        initial
            .as_ref()
            .and_then(|c| c.email.clone())
            .unwrap_or_default(),
    );
    let phone = RwSignal::new(
        initial
            .as_ref()
            .and_then(|c| c.phone.clone())
            .unwrap_or_default(),
    );
    let note = RwSignal::new(
        initial
            .as_ref()
            .and_then(|c| c.note.clone())
            .unwrap_or_default(),
    );
    let error = RwSignal::new(None::<String>);

    let create = Action::new(
        move |(client_id, request): &(String, CreateContactRequest)| {
            let (client_id, request) = (client_id.clone(), request.clone());
            async move { add_contact(client_id, request).await }
        },
    );
    let update = Action::new(move |(id, request): &(String, UpdateContactRequest)| {
        let (id, request) = (id.clone(), request.clone());
        async move { update_contact(id, request).await }
    });
    let finished = move |result: Option<Result<Reply<ContactDto>, ServerFnError>>| {
        if let Some(result) = result {
            match result {
                Ok(Reply::Ok(_)) => {
                    error.set(None);
                    version.update(|v| *v += 1);
                    on_done.run(());
                }
                Ok(other) => error.set(other.failure()),
                Err(_) => error.set(Some("Не удалось сохранить контакт.".into())),
            }
        }
    };
    Effect::new(move |_| finished(create.value().get()));
    Effect::new(move |_| finished(update.value().get()));

    let pending = move || create.pending().get() || update.pending().get();
    let submit_id = editing_id.clone();
    let parse_position = move || Uuid::parse_str(position.get().trim()).ok();

    view! {
        <form
            method="post"
            onsubmit="return false"
            data-testid="contact-form"
            on:submit=move |ev| {
                ev.prevent_default();
                match &submit_id {
                    Some(id) => {
                        update
                            .dispatch((
                                id.clone(),
                                UpdateContactRequest {
                                    full_name: Some(full_name.get()),
                                    position_id: Some(parse_position()),
                                    email: Some(opt(&email.get())),
                                    phone: Some(opt(&phone.get())),
                                    note: Some(opt(&note.get())),
                                },
                            ));
                    }
                    None => {
                        create
                            .dispatch((
                                client_id.clone(),
                                CreateContactRequest {
                                    full_name: full_name.get(),
                                    position_id: parse_position(),
                                    email: opt(&email.get()),
                                    phone: opt(&phone.get()),
                                    note: opt(&note.get()),
                                },
                            ));
                    }
                }
            }
        >
            <div class="form-grid">
                <label>
                    "ФИО"
                    <input type="text" required prop:value=move || full_name.get() data-testid="contact-fullname" on:input=move |ev| full_name.set(event_target_value(&ev)) />
                </label>
                <label>
                    "Должность"
                    <Dropdown
                        value=position
                        options=Signal::stored(
                            positions.iter().map(|p| (p.id.to_string(), p.name.clone())).collect::<Vec<_>>(),
                        )
                        empty="Не указана"
                        testid="contact-position-select"
                    />
                </label>
                <label>
                    "Email"
                    <input type="text" prop:value=move || email.get() data-testid="contact-email" on:input=move |ev| email.set(event_target_value(&ev)) />
                </label>
                <label>
                    "Телефон"
                    <input type="text" prop:value=move || phone.get() data-testid="contact-phone" on:input=move |ev| phone.set(event_target_value(&ev)) />
                </label>
                <label>
                    "Примечание"
                    <input type="text" prop:value=move || note.get() data-testid="contact-note" on:input=move |ev| note.set(event_target_value(&ev)) />
                </label>
            </div>
            {move || error.get().map(|e| view! { <div data-testid="contact-error"><Notice kind=NoticeKind::Error>{e}</Notice></div> })}
            <p>
                <button type="submit" class="btn btn--primary" data-testid="contact-submit" disabled=pending>
                    "Сохранить контакт"
                </button>
                <button type="button" class="btn btn--ghost" data-testid="contact-cancel" on:click=move |_| on_done.run(())>
                    "Отмена"
                </button>
            </p>
        </form>
    }
}
