//! Форма создания кандидата с предупреждением о дубле.
use candidate_contracts::{DuplicateInfo, PersonDto};
use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_navigate};
use recruitment_contracts::Stage;
use ui_kit::{Dropdown, Notice, NoticeKind, PageHeader, TextField};

use crate::funnel::server::{create_application, list_vacancies};
use ui_kit::Reply;

use super::server::{CreatePerson, person_action};

/// Сообщение об ошибке у конкретного поля.
fn field_error(
    outcome: Option<&Result<Reply<PersonDto>, ServerFnError>>,
    field: &str,
) -> Option<String> {
    match outcome {
        Some(Ok(Reply::Invalid { field: f, message })) if f == field => Some(message.clone()),
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
    let stages: Vec<Stage> = Stage::ALL
        .into_iter()
        .filter(|s| s.is_active() || *s == Stage::Hired)
        .collect();
    let vacancy = RwSignal::new(String::new());
    let stage = RwSignal::new(
        stages
            .first()
            .map(|s| s.key().to_string())
            .unwrap_or_default(),
    );
    let stage_options = Signal::stored(
        stages
            .iter()
            .map(|s| (s.key().to_string(), s.label().to_string()))
            .collect::<Vec<_>>(),
    );
    let vacancies = LocalResource::new(list_vacancies);
    let vacancy_options = Signal::derive(move || {
        vacancies
            .get()
            .and_then(Result::ok)
            .and_then(Reply::ok)
            .map(|list| {
                list.into_iter()
                    .map(|v| (v.id.to_string(), v.title))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    let link_error = RwSignal::new(None::<(String, String)>);

    // подтверждение действует на одну отправку
    Effect::new(move |_| match action.value().get() {
        Some(Ok(Reply::Ok(person))) => {
            let vacancy = vacancy.get_untracked();
            if vacancy.is_empty() {
                navigate("/candidates", Default::default());
            } else {
                let stage = stage.get_untracked();
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
        Some(Ok(Reply::Duplicate { existing, .. })) => {
            serde_json::from_value::<DuplicateInfo>(existing).ok()
        }
        _ => None,
    };
    let general_error = move || match outcome() {
        Some(Ok(Reply::Invalid { field, message })) if field.is_empty() => Some(message),
        Some(Ok(Reply::Forbidden)) => {
            Some("Недостаточно прав для создания кандидатов.".to_string())
        }
        Some(Ok(Reply::Unavailable | Reply::Conflict(_) | Reply::NotFound)) | Some(Err(_)) => {
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
                <TextField label="ФИО" name="full_name" required=true error=error("full_name") />
                <TextField label="Телефон" name="phone" placeholder="+7 916 123-45-67" error=error("phone") />
                <TextField label="Email" name="email" error=error("email") />
                <TextField label="Telegram" name="telegram" placeholder="@ник" error=error("telegram") />
                <TextField label="WhatsApp" name="whatsapp" error=error("whatsapp") />
                <TextField label="Источник" name="source" placeholder="hh.ru, реферал…" error=error("source") />
                <TextField label="Город" name="city" error=error("city") />
                <TextField
                    label="Ожидаемая зарплата, ₽"
                    name="expected_salary"
                    error=error("expected_salary")
                />
                <TextField
                    label="Стаж, лет (если нет истории работы)"
                    name="declared_experience_years"
                    error=error("declared_experience_years")
                />
                <TextField label="Навыки через запятую" name="skills" error=error("skills") />
            </div>
            <label>
                "Кратко о кандидате" <textarea name="summary" rows="3"></textarea>
            </label>
            <div class="form-grid">
                <label>
                    "Вакансия"
                    <Dropdown value=vacancy options=vacancy_options empty="Без вакансии" testid="vacancy" />
                </label>
                <label>
                    "Этап"
                    <Dropdown value=stage options=stage_options testid="stage" />
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
