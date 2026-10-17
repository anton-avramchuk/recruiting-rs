//! Боковая панель человека: все его отклики, профиль, заметка, история и действия.
use leptos::prelude::*;
use recruitment_contracts::{ApplicationDetailDto, ApplicationDto, HR_SYSTEMS, Stage};
use serde_json::{Value, json};
use ui_kit::{Drawer, Dropdown, Reply};

use super::{
    FunnelCtx,
    format::{browser_timezone_offset, client_line, local_to_rfc3339, short_moment, time_label},
    keys::{PanelKey, panel_key},
    server::{application_action, create_application, get_application, save_note},
};
use crate::{
    candidates::{
        format::{format_salary, years_label},
        server::get_person,
    },
    palette::view::{focus_when_ready, overlay_check, typing_in_a_field, with_press},
};

type Act = Action<(String, String, Value), Result<Reply<ApplicationDto>, ServerFnError>>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Idle,
    Interview,
    Reject,
    Handoff,
    Vacancy,
}

fn dash(value: Option<String>) -> String {
    value
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "—".into())
}

#[component]
pub fn PersonPanel(ctx: FunnelCtx, #[prop(into)] writable: Signal<bool>) -> impl IntoView {
    let detail = Resource::new(
        move || (ctx.open.get(), ctx.version.get()),
        |(id, _)| async move {
            match id {
                Some(id) => Some(get_application(id).await),
                None => None,
            }
        },
    );
    let message = RwSignal::new(None::<String>);

    // действия над откликом; результат применяется один раз
    let act: Act = Action::new(move |(id, action, body): &(String, String, Value)| {
        let (id, action, body) = (id.clone(), action.clone(), body.clone());
        async move { application_action(id, action, body).await }
    });
    Effect::new(move |_| {
        if let Some(result) = act.value().get() {
            match result {
                Ok(reply) => match reply.failure() {
                    None => {
                        message.set(None);
                        ctx.bump();
                    }
                    Some(text) => message.set(Some(text)),
                },
                Err(_) => message.set(Some("Не удалось выполнить действие.".into())),
            }
        }
    });

    // Esc закрывает панель, пока человек ещё грузится; с загруженным содержимым им занимается PanelBody
    Effect::new(move |_| {
        window_event_listener(leptos::ev::keydown, move |ev| {
            let loaded = document()
                .query_selector("[data-testid=panel-name]")
                .ok()
                .flatten()
                .is_some();
            if ev.key() == "Escape" && !loaded && !ev.default_prevented() {
                ctx.open.set(None);
            }
        });
    });

    view! {
        <Drawer
            open=Signal::derive(move || ctx.open.get().is_some())
            on_close=Callback::new(move |()| ctx.open.set(None))
            label="Человек"
            testid="panel"
        >
            <Transition fallback=|| {
                    view! { <p class="drawer__body muted">"Загрузка…"</p> }
                }>
                    {move || {
                        detail
                            .get()
                            .flatten()
                            .map(|result| match result {
                                Ok(Reply::Ok(d)) => {
                                    view! { <PanelBody detail=d ctx=ctx writable=writable act=act message=message /> }
                                        .into_any()
                                }
                                Ok(other) => {
                                    let text = other.failure().unwrap_or_default();
                                    view! {
                                        <div class="drawer__body">
                                            <p class="notice notice--error">{text}</p>
                                            <button class="btn" on:click=move |_| ctx.open.set(None)>
                                                "Закрыть"
                                            </button>
                                        </div>
                                    }
                                        .into_any()
                                }
                                Err(_) => {
                                    view! {
                                        <div class="drawer__body">
                                            <p class="notice notice--error">"Не удалось загрузить отклик."</p>
                                        </div>
                                    }
                                        .into_any()
                                }
                            })
                    }}
            </Transition>
        </Drawer>
    }
}

#[component]
fn PanelBody(
    detail: ApplicationDetailDto,
    ctx: FunnelCtx,
    writable: Signal<bool>,
    act: Act,
    message: RwSignal<Option<String>>,
) -> impl IntoView {
    let app = detail.application.clone();
    let id = app.id.to_string();
    let stage = app.stage;
    let mode = RwSignal::new(Mode::Idle);

    let dispatch = {
        let id = id.clone();
        move |action: &str, body: Value| {
            act.dispatch((id.clone(), action.to_string(), body));
        }
    };

    // клавиши панели: этап цифрой, E/R/S/N; Esc закрывает форму, затем панель
    let can_next_key = stage.next().is_some();
    let key_dispatch = dispatch.clone();
    let overlay_open = overlay_check();
    Effect::new(move |_| {
        let dispatch = key_dispatch.clone();
        window_event_listener(leptos::ev::keydown, move |ev| {
            if ev.ctrl_key() || ev.meta_key() || ev.alt_key() || ev.default_prevented() {
                return;
            }
            if ev.key() == "Escape" {
                ev.prevent_default();
                if mode.get_untracked() != Mode::Idle {
                    mode.set(Mode::Idle);
                } else {
                    ctx.open.set(None);
                }
                return;
            }
            if overlay_open() || typing_in_a_field(&ev) || !writable.get_untracked() {
                return;
            }
            let Some(key) = with_press(&ev, panel_key) else {
                return;
            };
            if act.pending().get_untracked() {
                return;
            }
            match key {
                PanelKey::Advance if can_next_key => dispatch("next", json!({})),
                PanelKey::Advance => return,
                PanelKey::Reject => {
                    mode.set(Mode::Reject);
                    focus_when_ready("[data-testid=reject-reason]", 5);
                }
                PanelKey::Interview => {
                    mode.set(Mode::Interview);
                    focus_when_ready("[data-testid=interview-at]", 5);
                }
                PanelKey::Note => focus_when_ready("[data-testid=note]", 0),
                PanelKey::Stage(n) => match Stage::ALL.get(usize::from(n) - 1) {
                    Some(s) if *s != stage => dispatch("stage", json!({"stage": s.key()})),
                    _ => return,
                },
            }
            ev.prevent_default();
        });
    });

    let stage_buttons = Stage::ALL
        .into_iter()
        .map(|s| {
            let dispatch = dispatch.clone();
            view! {
                <button
                    type="button"
                    aria-pressed=(s == stage).to_string()
                    title=s.label()
                    data-testid=format!("stage-{}", s.number())
                    disabled=move || !writable.get() || act.pending().get()
                    on:click=move |_| {
                        if s != stage {
                            dispatch("stage", json!({"stage": s.key()}));
                        }
                    }
                >
                    {s.number()}
                </button>
            }
        })
        .collect_view();

    let person_apps = detail
        .person_applications
        .iter()
        .map(|a| {
            let current = a.id == app.id;
            let other = a.id.to_string();
            let title = a.vacancy_title.clone();
            let label = a.stage.label();
            let match_text = a
                .match_percent
                .map(|m| format!("{m}%"))
                .unwrap_or_else(|| "—".into());
            let time = time_label(a.hours_on_stage);
            let urgent = a.overdue;
            view! {
                <button
                    type="button"
                    aria-current=current.to_string()
                    data-testid="person-app"
                    on:click=move |_| ctx.open.set(Some(other.clone()))
                >
                    <span>{title}</span>
                    <span class="mono">
                        {label} " · " {match_text} " · "
                        <span class:urgent=urgent>{time}</span>
                    </span>
                </button>
            }
        })
        .collect_view();

    let history = detail
        .history
        .iter()
        .map(|e| {
            view! {
                <li>
                    <time>{short_moment(&e.at)}</time>
                    {e.text.clone()}
                </li>
            }
        })
        .collect_view();

    let person_id = app.person_id.to_string();
    let comment_person = person_id.clone();
    let can_next = stage.next().is_some();
    let hired = stage == Stage::Hired;

    let interview_at = RwSignal::new(String::new());
    let reason = RwSignal::new(String::new());
    let system = RwSignal::new(HR_SYSTEMS[0].to_string());

    let dispatch_next = dispatch.clone();
    let controls = {
        let dispatch = dispatch.clone();
        let dispatch_reject = dispatch.clone();
        let dispatch_handoff = dispatch.clone();
        let person_id = person_id.clone();
        move || {
            let dispatch = dispatch.clone();
            let dispatch_reject = dispatch_reject.clone();
            let dispatch_handoff = dispatch_handoff.clone();
            let person_id = person_id.clone();
            match mode.get() {
                Mode::Idle => ().into_any(),
                Mode::Interview => view! {
                    <div class="inline-form">
                        <input
                            type="datetime-local"
                            data-testid="interview-at"
                            aria-label="Дата и время интервью"
                            on:input=move |ev| interview_at.set(event_target_value(&ev))
                        />
                        <button
                            class="btn btn--primary"
                            data-testid="interview-confirm"
                            on:click=move |_| {
                                match local_to_rfc3339(&interview_at.get(), browser_timezone_offset()) {
                                    Some(at) => {
                                        dispatch("interviews", json!({"at": at}));
                                        mode.set(Mode::Idle);
                                    }
                                    None => message.set(Some("Выберите дату и время интервью.".into())),
                                }
                            }
                        >
                            "Назначить"
                        </button>
                    </div>
                }
                .into_any(),
                Mode::Reject => view! {
                    <div class="inline-form">
                        <input
                            type="text"
                            placeholder="Причина (необязательно)"
                            data-testid="reject-reason"
                            on:input=move |ev| reason.set(event_target_value(&ev))
                        />
                        <button
                            class="btn btn--primary"
                            data-testid="reject-confirm"
                            on:click=move |_| {
                                dispatch_reject("reject", json!({"reason": reason.get()}));
                                mode.set(Mode::Idle);
                            }
                        >
                            "Отказать с письмом"
                        </button>
                    </div>
                }
                .into_any(),
                Mode::Handoff => view! {
                    <div class="inline-form">
                        <Dropdown
                            value=system
                            options=Signal::stored(
                                HR_SYSTEMS.iter().map(|s| ((*s).to_string(), (*s).to_string())).collect::<Vec<_>>(),
                            )
                            testid="handoff-system"
                        />
                        <button
                            class="btn btn--primary"
                            data-testid="handoff-confirm"
                            on:click=move |_| {
                                dispatch_handoff("handoff", json!({"system": system.get()}));
                                mode.set(Mode::Idle);
                            }
                        >
                            "Передать карточку"
                        </button>
                    </div>
                }
                .into_any(),
                Mode::Vacancy => view! { <AddToVacancy person_id=person_id ctx=ctx /> }.into_any(),
            }
        }
    };

    view! {
        <div class="drawer__head">
            <div>
                <h2 data-testid="panel-name">{app.full_name.clone()}</h2>
                <p class="muted">{app.vacancy_title.clone()}</p>
                {client_line(app.client_name.as_deref(), app.contact_name.as_deref(), None)
                    .map(|t| view! { <p class="muted" data-testid="panel-client">{t}</p> })}
            </div>
            <button class="btn btn--ghost" aria-label="Закрыть" data-testid="panel-close" on:click=move |_| ctx.open.set(None)>
                "✕"
            </button>
        </div>
        <div class="drawer__body">
            {move || {
                message.get().map(|m| view! { <p class="notice notice--error" data-testid="panel-error">{m}</p> })
            }}
            <section>
                <h2>"Этап: " <span data-testid="panel-stage">{stage.label()}</span></h2>
                <div class="stage-row">{stage_buttons}</div>
                <p class="muted mono">
                    "на этапе " <span class:urgent=app.overdue>{time_label(app.hours_on_stage)}</span>
                    {app.match_percent.map(|m| format!(" · совпадение {m}%"))}
                </p>
            </section>
            <section>
                <h2>"Отклики человека"</h2>
                <div class="person-apps">{person_apps}</div>
            </section>
            <PersonInfo person_id=person_id.clone() writable=writable />
            <section>
                <h2>"История"</h2>
                <ul class="history" data-testid="history">{history}</ul>
            </section>
            <crate::comments::Comments
                target_type="person"
                target_id=Signal::derive(move || comment_person.clone())
            />
        </div>
        <Show when=move || writable.get()>
            <div class="drawer__foot">
                {controls.clone()}
                <button
                    class="btn btn--primary"
                    data-testid="act-next"
                    disabled=move || !can_next || act.pending().get()
                    on:click={
                        let dispatch_next = dispatch_next.clone();
                        move |_| dispatch_next("next", json!({}))
                    }
                >
                    "Дальше по воронке " <kbd>"E"</kbd>
                </button>
                <button class="btn" data-testid="act-interview" on:click=move |_| mode.set(Mode::Interview)>
                    "Интервью " <kbd>"S"</kbd>
                </button>
                <button class="btn" data-testid="act-reject" on:click=move |_| mode.set(Mode::Reject)>
                    "Отказ " <kbd>"R"</kbd>
                </button>
                <button class="btn" data-testid="act-vacancy" on:click=move |_| mode.set(Mode::Vacancy)>
                    "На вакансию"
                </button>
                {hired
                    .then(|| {
                        view! {
                            <button class="btn" data-testid="act-handoff" on:click=move |_| mode.set(Mode::Handoff)>
                                "Передать в кадровую систему"
                            </button>
                        }
                    })}
            </div>
        </Show>
    }
}

/// Добавляет человека на ещё одну вакансию.
#[component]
fn AddToVacancy(person_id: String, ctx: FunnelCtx) -> impl IntoView {
    let vacancy_choices = crate::options::vacancy_options();
    let chosen = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let action = Action::new(move |(person, vacancy): &(String, String)| {
        let (person, vacancy) = (person.clone(), vacancy.clone());
        async move { create_application(person, vacancy, String::new()).await }
    });
    Effect::new(move |_| {
        if let Some(result) = action.value().get() {
            match result {
                Ok(Reply::Ok(created)) => {
                    error.set(None);
                    ctx.open.set(Some(created.id.to_string()));
                    ctx.bump();
                }
                Ok(other) => error.set(other.failure()),
                Err(_) => error.set(Some("Не удалось добавить на вакансию.".into())),
            }
        }
    });
    view! {
        <div class="inline-form">
            <Dropdown
                value=chosen
                options=vacancy_choices
                empty="Выберите вакансию"
                testid="add-vacancy-select"
            />
            <button
                class="btn btn--primary"
                data-testid="add-vacancy-confirm"
                disabled=move || chosen.get().is_empty() || action.pending().get()
                on:click={
                    let person_id = person_id.clone();
                    move |_| {
                        action.dispatch((person_id.clone(), chosen.get()));
                    }
                }
            >
                "Добавить"
            </button>
            {move || error.get().map(|e| view! { <span class="field-error">{e}</span> })}
        </div>
    }
}

/// Профиль человека из сервиса кандидатов и редактируемая заметка.
#[component]
fn PersonInfo(person_id: String, writable: Signal<bool>) -> impl IntoView {
    let person = Resource::new(
        {
            let person_id = person_id.clone();
            move || person_id.clone()
        },
        get_person,
    );
    view! {
        <Transition fallback=|| view! { <p class="muted">"Загрузка профиля…"</p> }>
            {move || {
                person
                    .get()
                    .map(|result| match result {
                        Ok(Reply::Ok(p)) => {
                            let note = RwSignal::new(p.note.clone().unwrap_or_default());
                            let saved = RwSignal::new(false);
                            let note_error = RwSignal::new(None::<String>);
                            let id = p.id.to_string();
                            let save = Action::new(move |text: &String| {
                                let (id, text) = (id.clone(), text.clone());
                                async move { save_note(id, text).await }
                            });
                            Effect::new(move |_| {
                                if let Some(result) = save.value().get() {
                                    match result {
                                        Ok(Reply::Ok(())) => {
                                            note_error.set(None);
                                            saved.set(true);
                                        }
                                        Ok(other) => note_error.set(other.failure()),
                                        Err(_) => note_error.set(Some("Не удалось сохранить заметку.".into())),
                                    }
                                }
                            });
                            let employments = p
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
                                            ", "
                                            {e.company.clone()}
                                            <span class="muted">{format!(" · {period}")}</span>
                                        </li>
                                    }
                                })
                                .collect_view();
                            let educations = p
                                .educations
                                .iter()
                                .map(|e| {
                                    view! {
                                        <li>
                                            {e.institution.clone()}
                                            {e.specialty.clone().map(|s| format!(", {s}"))}
                                        </li>
                                    }
                                })
                                .collect_view();
                            view! {
                                <section data-testid="panel-profile">
                                    <h2>"Профиль"</h2>
                                    <dl class="facts">
                                        <dt>"Телефон"</dt>
                                        <dd>{dash(p.phone.clone())}</dd>
                                        <dt>"Email"</dt>
                                        <dd>{dash(p.email.clone())}</dd>
                                        <dt>"Telegram"</dt>
                                        <dd>{dash(p.telegram.clone())}</dd>
                                        <dt>"Источник"</dt>
                                        <dd>{dash(p.source.clone())}</dd>
                                        <dt>"Город"</dt>
                                        <dd>{dash(p.city.clone())}</dd>
                                        <dt>"Ожидания"</dt>
                                        <dd>{dash(p.expected_salary.map(format_salary))}</dd>
                                        <dt>"Стаж"</dt>
                                        <dd>{years_label(p.experience_years)}</dd>
                                        <dt>"Навыки"</dt>
                                        <dd>{dash(Some(p.skills.join(", ")))}</dd>
                                    </dl>
                                    {p.summary.clone().map(|s| view! { <p>{s}</p> })}
                                    <h2>"Опыт"</h2>
                                    <ul class="history">{employments}</ul>
                                    <h2>"Образование"</h2>
                                    <ul class="history">{educations}</ul>
                                    <label>
                                        "Заметка"
                                        <textarea
                                            rows="3"
                                            data-testid="note"
                                            prop:value=move || note.get()
                                            readonly=move || !writable.get()
                                            on:input=move |ev| {
                                                note.set(event_target_value(&ev));
                                                saved.set(false);
                                            }
                                            on:change=move |_| {
                                                if writable.get_untracked() {
                                                    save.dispatch(note.get_untracked());
                                                }
                                            }
                                        ></textarea>
                                    </label>
                                    {move || {
                                        saved
                                            .get()
                                            .then(|| view! { <span class="muted" data-testid="note-saved">"Заметка сохранена"</span> })
                                    }}
                                    {move || note_error.get().map(|e| view! { <span class="field-error">{e}</span> })}
                                </section>
                            }
                                .into_any()
                        }
                        _ => {
                            view! {
                                <p class="muted">"Профиль человека сейчас недоступен."</p>
                            }
                                .into_any()
                        }
                    })
            }}
        </Transition>
    }
}
