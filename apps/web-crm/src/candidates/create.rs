//! Форма создания кандидата с предупреждением о дубле.
use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_navigate};
use recruitment_contracts::Stage;
use ui_kit::{Notice, NoticeKind, PageHeader};

use crate::funnel::server::{create_application, list_vacancies};
use ui_kit::Reply;

use super::{
    fields::Field,
    outcome::CreateOutcome,
    server::{CreatePerson, person_action},
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
    let action = person_action(CreatePerson::run);
    // скрытое поле читается формой синхронно в момент отправки, поэтому значение пишем в DOM напрямую
    let confirm_input = NodeRef::<leptos::html::Input>::new();
    let form = NodeRef::<leptos::html::Form>::new();
    let navigate = use_navigate();
    // вакансия и этап необязательны: если вакансия выбрана, после создания кандидата создаётся отклик
    let vacancy_select = NodeRef::<leptos::html::Select>::new();
    let stage_select = NodeRef::<leptos::html::Select>::new();
    let vacancies = Resource::new(|| (), |_| list_vacancies());
    let link_error = RwSignal::new(None::<(String, String)>);

    // подтверждение действует на одну отправку
    Effect::new(move |_| match action.value().get() {
        Some(Ok(CreateOutcome::Created(person))) => {
            let vacancy = vacancy_select.get().map(|s| s.value()).unwrap_or_default();
            if vacancy.is_empty() {
                navigate("/candidates", Default::default());
            } else {
                let stage = stage_select.get().map(|s| s.value()).unwrap_or_default();
                let id = person.id.to_string();
                let navigate = navigate.clone();
                leptos::task::spawn_local(async move {
                    match create_application(id.clone(), vacancy, stage).await {
                        Ok(Reply::Ok(_)) => navigate("/applications", Default::default()),
                        Ok(other) => {
                            link_error.set(Some((id, other.failure().unwrap_or_default())))
                        }
                        Err(_) => link_error.set(Some((id, "Сервис воронки недоступен.".into()))),
                    }
                });
            }
        }
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

        <form
            node_ref=form
            class="card"
            method="post"
            onsubmit="return false"
            on:submit=move |ev| {
                ev.prevent_default();
                action.dispatch(CreatePerson::from_fields(&ui_kit::form_fields(&ev)));
            }
        >
            <div class="form-grid">
                <Field label="ФИО" name="full_name" required=true error=error("full_name") />
                <Field label="Телефон" name="phone" placeholder="+7 916 123-45-67" error=error("phone") />
                <Field label="Email" name="email" error=error("email") />
                <Field label="Telegram" name="telegram" placeholder="@ник" error=error("telegram") />
                <Field label="WhatsApp" name="whatsapp" error=error("whatsapp") />
                <Field label="Источник" name="source" placeholder="hh.ru, реферал…" error=error("source") />
                <Field label="Город" name="city" error=error("city") />
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
            <div class="form-grid">
                <label>
                    "Вакансия"
                    <select node_ref=vacancy_select data-testid="vacancy">
                        <option value="">"Без вакансии"</option>
                        <Transition fallback=|| ()>
                            {move || {
                                vacancies
                                    .get()
                                    .and_then(Result::ok)
                                    .and_then(Reply::ok)
                                    .map(|list| {
                                        list.into_iter()
                                            .map(|v| view! { <option value=v.id.to_string()>{v.title}</option> })
                                            .collect_view()
                                    })
                            }}
                        </Transition>
                    </select>
                </label>
                <label>
                    "Этап"
                    <select node_ref=stage_select data-testid="stage">
                        {Stage::ALL
                            .into_iter()
                            .filter(|s| s.is_active() || *s == Stage::Hired)
                            .map(|s| view! { <option value=s.key()>{s.label()}</option> })
                            .collect_view()}
                    </select>
                </label>
            </div>
            {move || {
                link_error
                    .get()
                    .map(|(id, text)| {
                        view! {
                            <div class="notice notice--error" role="alert" data-testid="link-error">
                                <p>
                                    {format!("Кандидат создан, но отклик не создан: {text} ")}
                                    <A href=format!("/candidates/{id}")>"Открыть карточку кандидата"</A>
                                </p>
                            </div>
                        }
                    })
            }}
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
        </form>
    }
}
