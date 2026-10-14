//! Форма создания кандидата с предупреждением о дубле.
use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_navigate};
use ui_kit::{Notice, NoticeKind, PageHeader};

use super::{
    fields::Field,
    outcome::CreateOutcome,
    server::{CreatePerson, create_person},
};

/// Сообщение об ошибке у конкретного поля.
fn field_error(
    outcome: Option<&Result<CreateOutcome, ServerFnError>>,
    field: &str,
) -> Option<String> {
    match outcome {
        Some(Ok(CreateOutcome::Invalid { field: f, message })) if f == field => {
            Some(message.clone())
        }
        _ => None,
    }
}

fn matched_by_label(matched_by: &str) -> &'static str {
    match matched_by {
        "email" => "по email",
        "phone" => "по телефону",
        _ => "по имени",
    }
}

#[component]
pub fn CreateCandidatePage() -> impl IntoView {
    let action = ServerAction::<CreatePerson>::new();
    // скрытое поле читается формой синхронно в момент отправки, поэтому значение пишем в DOM напрямую
    let confirm_input = NodeRef::<leptos::html::Input>::new();
    let form = NodeRef::<leptos::html::Form>::new();
    let navigate = use_navigate();

    // подтверждение действует на одну отправку
    Effect::new(move |_| match action.value().get() {
        Some(Ok(CreateOutcome::Created(_))) => navigate("/candidates", Default::default()),
        Some(_) => {
            if let Some(input) = confirm_input.get() {
                input.set_value("");
            }
        }
        None => {}
    });

    let outcome = move || action.value().get();
    let error =
        move |field: &'static str| Signal::derive(move || field_error(outcome().as_ref(), field));
    let duplicate = move || match outcome() {
        Some(Ok(CreateOutcome::Duplicate(d))) => Some(d),
        _ => None,
    };
    let general_error = move || match outcome() {
        Some(Ok(CreateOutcome::Invalid { field, message })) if field.is_empty() => Some(message),
        Some(Ok(CreateOutcome::Forbidden)) => {
            Some("Недостаточно прав для создания кандидатов.".to_string())
        }
        Some(Ok(CreateOutcome::Unavailable)) | Some(Err(_)) => {
            Some("Не удалось создать кандидата. Попробуйте позже.".to_string())
        }
        _ => None,
    };

    view! {
        <PageHeader title="Новый кандидат">
            <A href="/candidates" attr:class="btn">
                "К списку"
            </A>
        </PageHeader>

        <ActionForm action=action node_ref=form attr:class="card">
            <div class="form-grid">
                <Field label="ФИО" name="full_name" required=true error=error("full_name") />
                <Field label="Телефон" name="phone" placeholder="+7 916 123-45-67" error=error("phone") />
                <Field label="Email" name="email" error=error("email") />
                <Field label="Telegram" name="telegram" placeholder="@ник" error=error("telegram") />
                <Field label="WhatsApp" name="whatsapp" error=error("whatsapp") />
                <Field label="Источник" name="source" placeholder="hh.ru, реферал…" error=error("source") />
                <Field
                    label="Ожидаемая зарплата, ₽"
                    name="expected_salary"
                    error=error("expected_salary")
                />
                <Field
                    label="Стаж, лет (если нет истории работы)"
                    name="declared_experience_years"
                    error=error("declared_experience_years")
                />
                <Field label="Навыки через запятую" name="skills" error=error("skills") />
            </div>
            <label>
                "Кратко о кандидате" <textarea name="summary" rows="3"></textarea>
            </label>
            <input type="hidden" name="confirm_duplicate" value="" node_ref=confirm_input />

            {move || {
                duplicate()
                    .map(|d| {
                        let how = matched_by_label(&d.matched_by);
                        view! {
                            <div class="notice notice--warn" role="alert" data-testid="duplicate-banner">
                                <p>
                                    "Возможно, это уже есть в базе: "
                                    <A href=format!("/candidates/{}", d.id)>{d.full_name.clone()}</A>
                                    {format!(" (совпадение {how})")}
                                </p>
                                <button
                                    type="button"
                                    class="btn"
                                    data-testid="duplicate-confirm"
                                    on:click=move |_| {
                                        if let Some(input) = confirm_input.get() {
                                            input.set_value("true");
                                        }
                                        if let Some(form) = form.get() {
                                            let _ = form.request_submit();
                                        }
                                    }
                                >
                                    "Создать всё равно"
                                </button>
                                <button
                                    type="button"
                                    class="btn btn--ghost"
                                    data-testid="duplicate-cancel"
                                    on:click=move |_| {
                                        if let Some(input) = confirm_input.get() {
                                            input.set_value("");
                                        }
                                        action.value().set(None);
                                    }
                                >
                                    "Отмена"
                                </button>
                            </div>
                        }
                    })
            }}
            {move || {
                general_error().map(|m| view! { <Notice kind=NoticeKind::Error>{m}</Notice> })
            }}

            <p>
                <button
                    type="submit"
                    class="btn btn--primary"
                    data-testid="submit"
                    disabled=move || action.pending().get()
                >
                    {move || if action.pending().get() { "Создаём…" } else { "Создать кандидата" }}
                </button>
            </p>
        </ActionForm>
    }
}

#[allow(dead_code)]
fn _server_fn_is_linked() {
    let _ = create_person;
}
