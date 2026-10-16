//! Карточка кандидата: профиль и контакты, опыт работы, образование. Разделы правятся независимо.
//!
//! Состояние карточки живёт в одном сигнале: сохранение раздела обновляет его ответом сервера,
//! поэтому страница не перестраивается, а открытые соседние разделы не сбрасываются.
use candidate_contracts::{EducationDto, EmploymentDto, PersonDto};
use leptos::prelude::*;
use leptos_router::{
    components::A,
    hooks::{use_navigate, use_params_map},
};
use ui_kit::{EmptyState, Notice, NoticeKind, SessionCtx};

use super::{
    fields::Field,
    format::{format_salary, years_label},
    list::{can_delete, can_write},
    outcome::{DeleteOutcome, LoadOutcome, SaveOutcome},
    server::{
        DeletePerson, SaveEducations, SaveEmployments, UpdatePerson, get_person, person_action,
    },
};

fn field_error(
    outcome: Option<&Result<SaveOutcome, ServerFnError>>,
    field: &str,
) -> Option<String> {
    match outcome {
        Some(Ok(SaveOutcome::Invalid { field: f, message })) if f == field => Some(message.clone()),
        _ => None,
    }
}

/// Сообщение о неудаче сохранения, не привязанное к полю.
fn general_error(outcome: Option<&Result<SaveOutcome, ServerFnError>>) -> Option<&'static str> {
    match outcome {
        Some(Ok(SaveOutcome::Forbidden)) => Some("Недостаточно прав для изменения."),
        Some(Ok(SaveOutcome::NotFound)) => Some("Кандидат уже удалён."),
        Some(Ok(SaveOutcome::Unavailable)) | Some(Err(_)) => {
            Some("Не удалось сохранить. Попробуйте позже.")
        }
        _ => None,
    }
}

/// Любая ошибка редактора списка (с привязкой к полю или без) одной строкой.
fn list_error(outcome: Option<&Result<SaveOutcome, ServerFnError>>) -> Option<String> {
    match outcome {
        Some(Ok(SaveOutcome::Invalid { message, .. })) => Some(message.clone()),
        other => general_error(other).map(str::to_string),
    }
}

fn dash(value: Option<String>) -> String {
    value
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "—".into())
}

#[component]
pub fn CandidateCardPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.read().get("id").unwrap_or_default();
    let person = Resource::new(id, get_person);

    view! {
        <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
            {move || {
                person
                    .get()
                    .map(|result| match result {
                        Ok(LoadOutcome::Found(p)) => view! { <PersonCard person=*p /> }.into_any(),
                        Ok(LoadOutcome::NotFound) => {
                            view! {
                                <EmptyState
                                    title="Кандидат не найден"
                                    hint="Возможно, он удалён или принадлежит другой организации."
                                />
                                <p>
                                    <A href="/candidates">"К списку кандидатов"</A>
                                </p>
                            }
                                .into_any()
                        }
                        _ => {
                            view! {
                                <Notice kind=NoticeKind::Error>
                                    "Не удалось загрузить кандидата. Попробуйте позже."
                                </Notice>
                            }
                                .into_any()
                        }
                    })
            }}
        </Transition>
    }
}

#[component]
fn PersonCard(person: PersonDto) -> impl IntoView {
    let ctx = expect_context::<SessionCtx>();
    let writable = Signal::derive(move || can_write(ctx));
    let state = RwSignal::new(person);

    view! {
        <div class="page-header">
            <div>
                <h1>{move || state.get().full_name}</h1>
                <p class="muted" data-testid="experience">
                    {move || format!("Стаж: {}", years_label(state.get().experience_years))}
                </p>
            </div>
            <div class="page-header__actions">
                <A href="/candidates" attr:class="btn">
                    "К списку"
                </A>
                {move || can_delete(ctx).then(|| view! { <DeleteControl id=state.get_untracked().id.to_string() /> })}
            </div>
        </div>
        <ProfileSection state=state writable=writable />
        <EmploymentsSection state=state writable=writable />
        <EducationsSection state=state writable=writable />
        <crate::comments::Comments
            target_type="person"
            target_id=Signal::derive(move || state.get().id.to_string())
        />
    }
}

/// Удаление в два шага: кнопка, затем явное подтверждение.
#[component]
fn DeleteControl(id: String) -> impl IntoView {
    let confirming = RwSignal::new(false);
    let action = person_action(DeletePerson::run);
    let navigate = use_navigate();

    Effect::new(move |_| {
        if let Some(Ok(DeleteOutcome::Deleted | DeleteOutcome::NotFound)) = action.value().get() {
            navigate("/candidates", Default::default());
        }
    });
    let failed = move || match action.value().get() {
        Some(Ok(DeleteOutcome::Forbidden)) => Some("Недостаточно прав для удаления."),
        Some(Ok(DeleteOutcome::Unavailable)) | Some(Err(_)) => {
            Some("Не удалось удалить. Попробуйте позже.")
        }
        _ => None,
    };

    view! {
        {move || {
            if confirming.get() {
                let id = id.clone();
                view! {
                    <span class="notice notice--warn" data-testid="delete-panel">
                        "Удалить кандидата безвозвратно? Вместе с ним удалятся и его отклики. "
                        <button
                            class="btn btn--primary"
                            data-testid="delete-confirm"
                            disabled=move || action.pending().get()
                            on:click=move |_| {
                                action.dispatch(DeletePerson { id: id.clone() });
                            }
                        >
                            "Да, удалить"
                        </button>
                        <button
                            class="btn"
                            data-testid="delete-cancel"
                            on:click=move |_| {
                                confirming.set(false);
                                action.value().set(None);
                            }
                        >
                            "Отмена"
                        </button>
                        {move || failed().map(|m| view! { <span class="field-error">{m}</span> })}
                    </span>
                }
                    .into_any()
            } else {
                view! {
                    <button class="btn" data-testid="delete" on:click=move |_| confirming.set(true)>
                        "Удалить"
                    </button>
                }
                    .into_any()
            }
        }}
    }
}

// ---------- профиль и контакты ----------

#[component]
fn ProfileSection(state: RwSignal<PersonDto>, writable: Signal<bool>) -> impl IntoView {
    let editing = RwSignal::new(false);
    let action = person_action(UpdatePerson::run);

    Effect::new(move |_| {
        if let Some(Ok(SaveOutcome::Saved(saved))) = action.value().get() {
            state.set(*saved);
            editing.set(false);
        }
    });
    let outcome = move || action.value().get();
    let error =
        move |field: &'static str| Signal::derive(move || field_error(outcome().as_ref(), field));

    let view_mode = move || {
        let p = state.get();
        view! {
            <dl class="facts" data-testid="profile-view">
                <dt>"Телефон"</dt>
                <dd>{dash(p.phone.clone())}</dd>
                <dt>"Email"</dt>
                <dd>{dash(p.email.clone())}</dd>
                <dt>"Telegram"</dt>
                <dd>{dash(p.telegram.clone())}</dd>
                <dt>"WhatsApp"</dt>
                <dd>{dash(p.whatsapp.clone())}</dd>
                <dt>"Источник"</dt>
                <dd>{dash(p.source.clone())}</dd>
                <dt>"Город"</dt>
                <dd data-testid="profile-city">{dash(p.city.clone())}</dd>
                <dt>"Ожидаемая зарплата"</dt>
                <dd>{dash(p.expected_salary.map(format_salary))}</dd>
                <dt>"Навыки"</dt>
                <dd>{dash(Some(p.skills.join(", ")))}</dd>
                <dt>"О кандидате"</dt>
                <dd>{dash(p.summary.clone())}</dd>
                <dt>"Заметка"</dt>
                <dd data-testid="profile-note">{dash(p.note.clone())}</dd>
            </dl>
        }
    };

    let edit_mode = move || {
        let p = state.get_untracked();
        view! {
            <form
                data-testid="profile-form"
                method="post"
                onsubmit="return false"
                on:submit=move |ev| {
                    ev.prevent_default();
                    action.dispatch(UpdatePerson::from_fields(&ui_kit::form_fields(&ev)));
                }
            >
                <input type="hidden" name="id" value=p.id.to_string() />
                <div class="form-grid">
                    <Field
                        label="ФИО"
                        name="full_name"
                        value=p.full_name.clone()
                        required=true
                        error=error("full_name")
                    />
                    <Field label="Телефон" name="phone" value=p.phone.clone().unwrap_or_default() error=error("phone") />
                    <Field label="Email" name="email" value=p.email.clone().unwrap_or_default() error=error("email") />
                    <Field
                        label="Telegram"
                        name="telegram"
                        value=p.telegram.clone().unwrap_or_default()
                        error=error("telegram")
                    />
                    <Field
                        label="WhatsApp"
                        name="whatsapp"
                        value=p.whatsapp.clone().unwrap_or_default()
                        error=error("whatsapp")
                    />
                    <Field
                        label="Источник"
                        name="source"
                        value=p.source.clone().unwrap_or_default()
                        error=error("source")
                    />
                    <Field label="Город" name="city" value=p.city.clone().unwrap_or_default() error=error("city") />
                    <Field
                        label="Ожидаемая зарплата, ₽"
                        name="expected_salary"
                        value=p.expected_salary.map(|s| s.to_string()).unwrap_or_default()
                        error=error("expected_salary")
                    />
                    <Field
                        label="Навыки через запятую"
                        name="skills"
                        value=p.skills.join(", ")
                        error=error("skills")
                    />
                </div>
                <label>
                    "О кандидате"
                    <textarea name="summary" rows="3">
                        {p.summary.clone().unwrap_or_default()}
                    </textarea>
                </label>
                {move || {
                    general_error(outcome().as_ref())
                        .map(|m| view! { <Notice kind=NoticeKind::Error>{m}</Notice> })
                }}
                <p class="toolbar">
                    <button type="submit" class="btn btn--primary" data-testid="profile-save">
                        "Сохранить"
                    </button>
                    <button
                        type="button"
                        class="btn"
                        on:click=move |_| {
                            editing.set(false);
                            action.value().set(None);
                        }
                    >
                        "Отмена"
                    </button>
                </p>
            </form>
        }
    };

    view! {
        <section class="card section">
            <div class="section__head">
                <h2>"Контакты и профиль"</h2>
                {move || {
                    (writable.get() && !editing.get())
                        .then(|| {
                            view! {
                                <button class="btn" data-testid="edit-profile" on:click=move |_| editing.set(true)>
                                    "Изменить"
                                </button>
                            }
                        })
                }}
            </div>
            {move || if editing.get() { edit_mode().into_any() } else { view_mode().into_any() }}
        </section>
    }
}

// ---------- опыт работы ----------

#[derive(Clone)]
struct EmploymentRow {
    key: u32,
    company: RwSignal<String>,
    position: RwSignal<String>,
    start: RwSignal<String>,
    end: RwSignal<String>,
    description: RwSignal<String>,
}

impl EmploymentRow {
    fn new(key: u32, e: Option<&EmploymentDto>) -> Self {
        Self {
            key,
            company: RwSignal::new(e.map(|e| e.company.clone()).unwrap_or_default()),
            position: RwSignal::new(e.map(|e| e.position.clone()).unwrap_or_default()),
            start: RwSignal::new(e.map(|e| e.start.clone()).unwrap_or_default()),
            end: RwSignal::new(e.and_then(|e| e.end.clone()).unwrap_or_default()),
            description: RwSignal::new(e.and_then(|e| e.description.clone()).unwrap_or_default()),
        }
    }

    fn dto(&self) -> EmploymentDto {
        let text = |s: String| Some(s.trim().to_string()).filter(|s| !s.is_empty());
        EmploymentDto {
            company: self.company.get_untracked(),
            position: self.position.get_untracked(),
            start: self.start.get_untracked(),
            end: text(self.end.get_untracked()),
            description: text(self.description.get_untracked()),
        }
    }
}

#[component]
fn EmploymentsSection(state: RwSignal<PersonDto>, writable: Signal<bool>) -> impl IntoView {
    let editing = RwSignal::new(false);
    let action = person_action(SaveEmployments::run);
    let next_key = RwSignal::new(0u32);
    let rows = RwSignal::new(Vec::<EmploymentRow>::new());
    let id = state.get_untracked().id.to_string();
    // сигналы строк должны принадлежать секции, а не обработчику клика: иначе они умрут
    // вместе с областью кнопки, которая скрывается при входе в режим правки
    let owner = Owner::current().expect("section owner");

    let begin_edit = {
        let owner = owner.clone();
        move |_| {
            let initial = state.get_untracked().employments;
            let mut key = 0;
            rows.set(owner.with(|| {
                initial
                    .iter()
                    .map(|e| {
                        key += 1;
                        EmploymentRow::new(key, Some(e))
                    })
                    .collect()
            }));
            next_key.set(key);
            action.value().set(None);
            editing.set(true);
        }
    };

    Effect::new(move |_| {
        if let Some(Ok(SaveOutcome::Saved(saved))) = action.value().get() {
            state.set(*saved);
            editing.set(false);
        }
    });
    let outcome = move || action.value().get();

    view! {
        <section class="card section">
            <div class="section__head">
                <h2>"Опыт работы"</h2>
                {move || {
                    (writable.get() && !editing.get())
                        .then(|| {
                            view! {
                                <button class="btn" data-testid="edit-employments" on:click=begin_edit.clone()>
                                    "Изменить"
                                </button>
                            }
                        })
                }}
            </div>
            {move || {
                if editing.get() {
                    let id = id.clone();
                    let owner = owner.clone();
                    view! {
                        <div data-testid="employments-form">
                            <For each=move || rows.get() key=|r| r.key let:row>
                                <div class="row-card" data-testid="employment-row">
                                    <div class="form-grid">
                                        <label>
                                            "Компания"
                                            <input
                                                data-field="company"
                                                prop:value=move || row.company.get()
                                                on:input=move |ev| row.company.set(event_target_value(&ev))
                                            />
                                        </label>
                                        <label>
                                            "Должность"
                                            <input
                                                data-field="position"
                                                prop:value=move || row.position.get()
                                                on:input=move |ev| row.position.set(event_target_value(&ev))
                                            />
                                        </label>
                                        <label>
                                            "Начало"
                                            <input
                                                type="date"
                                                data-field="start"
                                                prop:value=move || row.start.get()
                                                on:input=move |ev| row.start.set(event_target_value(&ev))
                                            />
                                        </label>
                                        <label>
                                            "Окончание (пусто — по настоящее время)"
                                            <input
                                                type="date"
                                                data-field="end"
                                                prop:value=move || row.end.get()
                                                on:input=move |ev| row.end.set(event_target_value(&ev))
                                            />
                                        </label>
                                    </div>
                                    <label>
                                        "Описание"
                                        <input
                                            data-field="description"
                                            prop:value=move || row.description.get()
                                            on:input=move |ev| row.description.set(event_target_value(&ev))
                                        />
                                    </label>
                                    <button
                                        type="button"
                                        class="btn btn--ghost"
                                        on:click=move |_| rows.update(|r| r.retain(|x| x.key != row.key))
                                    >
                                        "Убрать"
                                    </button>
                                </div>
                            </For>
                            <button
                                type="button"
                                class="btn"
                                data-testid="add-employment"
                                on:click=move |_| {
                                    next_key.update(|k| *k += 1);
                                    let key = next_key.get_untracked();
                                    let row = owner.with(|| EmploymentRow::new(key, None));
                                    rows.update(|r| r.push(row));
                                }
                            >
                                "Добавить место работы"
                            </button>
                            {move || {
                                list_error(outcome().as_ref())
                                    .map(|m| {
                                        view! {
                                            <p class="field-error" role="alert" data-testid="error-employments">
                                                {m}
                                            </p>
                                        }
                                    })
                            }}
                            <p class="toolbar">
                                <button
                                    type="button"
                                    class="btn btn--primary"
                                    data-testid="employments-save"
                                    on:click=move |_| {
                                        let items = rows.get_untracked().iter().map(EmploymentRow::dto).collect();
                                        action.dispatch(SaveEmployments { id: id.clone(), items });
                                    }
                                >
                                    "Сохранить"
                                </button>
                                <button
                                    type="button"
                                    class="btn"
                                    data-testid="employments-cancel"
                                    on:click=move |_| {
                                        editing.set(false);
                                        action.value().set(None);
                                    }
                                >
                                    "Отмена"
                                </button>
                            </p>
                        </div>
                    }
                        .into_any()
                } else if state.get().employments.is_empty() {
                    view! { <p class="muted" data-testid="employments-view">"Опыт работы не указан."</p> }.into_any()
                } else {
                    view! {
                        <ul class="timeline" data-testid="employments-view">
                            {state
                                .get()
                                .employments
                                .iter()
                                .map(|e| {
                                    let period = format!(
                                        "{} — {}",
                                        e.start,
                                        e.end.clone().unwrap_or_else(|| "по настоящее время".into()),
                                    );
                                    view! {
                                        <li>
                                            <strong>{e.position.clone()}</strong>
                                            " · "
                                            {e.company.clone()}
                                            <br />
                                            <span class="muted">{period}</span>
                                            {e.description.clone().map(|d| view! { <p>{d}</p> })}
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ul>
                    }
                        .into_any()
                }
            }}
        </section>
    }
}

// ---------- образование ----------

#[derive(Clone)]
struct EducationRow {
    key: u32,
    institution: RwSignal<String>,
    specialty: RwSignal<String>,
    year_from: RwSignal<String>,
    year_to: RwSignal<String>,
}

impl EducationRow {
    fn new(key: u32, e: Option<&EducationDto>) -> Self {
        let year = |y: Option<i32>| y.map(|y| y.to_string()).unwrap_or_default();
        Self {
            key,
            institution: RwSignal::new(e.map(|e| e.institution.clone()).unwrap_or_default()),
            specialty: RwSignal::new(e.and_then(|e| e.specialty.clone()).unwrap_or_default()),
            year_from: RwSignal::new(year(e.and_then(|e| e.year_from))),
            year_to: RwSignal::new(year(e.and_then(|e| e.year_to))),
        }
    }

    fn dto(&self) -> EducationDto {
        let year = |s: String| s.trim().parse::<i32>().ok();
        EducationDto {
            institution: self.institution.get_untracked(),
            specialty: Some(self.specialty.get_untracked().trim().to_string())
                .filter(|s| !s.is_empty()),
            year_from: year(self.year_from.get_untracked()),
            year_to: year(self.year_to.get_untracked()),
        }
    }
}

#[component]
fn EducationsSection(state: RwSignal<PersonDto>, writable: Signal<bool>) -> impl IntoView {
    let editing = RwSignal::new(false);
    let action = person_action(SaveEducations::run);
    let next_key = RwSignal::new(0u32);
    let rows = RwSignal::new(Vec::<EducationRow>::new());
    let id = state.get_untracked().id.to_string();
    let owner = Owner::current().expect("section owner");

    let begin_edit = {
        let owner = owner.clone();
        move |_| {
            let initial = state.get_untracked().educations;
            let mut key = 0;
            rows.set(owner.with(|| {
                initial
                    .iter()
                    .map(|e| {
                        key += 1;
                        EducationRow::new(key, Some(e))
                    })
                    .collect()
            }));
            next_key.set(key);
            action.value().set(None);
            editing.set(true);
        }
    };

    Effect::new(move |_| {
        if let Some(Ok(SaveOutcome::Saved(saved))) = action.value().get() {
            state.set(*saved);
            editing.set(false);
        }
    });
    let outcome = move || action.value().get();

    view! {
        <section class="card section">
            <div class="section__head">
                <h2>"Образование"</h2>
                {move || {
                    (writable.get() && !editing.get())
                        .then(|| {
                            view! {
                                <button class="btn" data-testid="edit-educations" on:click=begin_edit.clone()>
                                    "Изменить"
                                </button>
                            }
                        })
                }}
            </div>
            {move || {
                if editing.get() {
                    let id = id.clone();
                    let owner = owner.clone();
                    view! {
                        <div data-testid="educations-form">
                            <For each=move || rows.get() key=|r| r.key let:row>
                                <div class="row-card" data-testid="education-row">
                                    <div class="form-grid">
                                        <label>
                                            "Учебное заведение"
                                            <input
                                                data-field="institution"
                                                prop:value=move || row.institution.get()
                                                on:input=move |ev| row.institution.set(event_target_value(&ev))
                                            />
                                        </label>
                                        <label>
                                            "Специальность"
                                            <input
                                                data-field="specialty"
                                                prop:value=move || row.specialty.get()
                                                on:input=move |ev| row.specialty.set(event_target_value(&ev))
                                            />
                                        </label>
                                        <label>
                                            "Год начала"
                                            <input
                                                data-field="year_from"
                                                prop:value=move || row.year_from.get()
                                                on:input=move |ev| row.year_from.set(event_target_value(&ev))
                                            />
                                        </label>
                                        <label>
                                            "Год окончания"
                                            <input
                                                data-field="year_to"
                                                prop:value=move || row.year_to.get()
                                                on:input=move |ev| row.year_to.set(event_target_value(&ev))
                                            />
                                        </label>
                                    </div>
                                    <button
                                        type="button"
                                        class="btn btn--ghost"
                                        on:click=move |_| rows.update(|r| r.retain(|x| x.key != row.key))
                                    >
                                        "Убрать"
                                    </button>
                                </div>
                            </For>
                            <button
                                type="button"
                                class="btn"
                                data-testid="add-education"
                                on:click=move |_| {
                                    next_key.update(|k| *k += 1);
                                    let key = next_key.get_untracked();
                                    let row = owner.with(|| EducationRow::new(key, None));
                                    rows.update(|r| r.push(row));
                                }
                            >
                                "Добавить образование"
                            </button>
                            {move || {
                                list_error(outcome().as_ref())
                                    .map(|m| {
                                        view! {
                                            <p class="field-error" role="alert" data-testid="error-educations">
                                                {m}
                                            </p>
                                        }
                                    })
                            }}
                            <p class="toolbar">
                                <button
                                    type="button"
                                    class="btn btn--primary"
                                    data-testid="educations-save"
                                    on:click=move |_| {
                                        let items = rows.get_untracked().iter().map(EducationRow::dto).collect();
                                        action.dispatch(SaveEducations { id: id.clone(), items });
                                    }
                                >
                                    "Сохранить"
                                </button>
                                <button
                                    type="button"
                                    class="btn"
                                    data-testid="educations-cancel"
                                    on:click=move |_| {
                                        editing.set(false);
                                        action.value().set(None);
                                    }
                                >
                                    "Отмена"
                                </button>
                            </p>
                        </div>
                    }
                        .into_any()
                } else if state.get().educations.is_empty() {
                    view! { <p class="muted" data-testid="educations-view">"Образование не указано."</p> }.into_any()
                } else {
                    view! {
                        <ul class="timeline" data-testid="educations-view">
                            {state
                                .get()
                                .educations
                                .iter()
                                .map(|e| {
                                    let years = match (e.year_from, e.year_to) {
                                        (Some(f), Some(t)) => format!("{f} — {t}"),
                                        (Some(f), None) => format!("с {f}"),
                                        (None, Some(t)) => format!("до {t}"),
                                        (None, None) => String::new(),
                                    };
                                    view! {
                                        <li>
                                            <strong>{e.institution.clone()}</strong>
                                            {e.specialty.clone().map(|s| format!(" · {s}"))}
                                            <br />
                                            <span class="muted">{years}</span>
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ul>
                    }
                        .into_any()
                }
            }}
        </section>
    }
}
