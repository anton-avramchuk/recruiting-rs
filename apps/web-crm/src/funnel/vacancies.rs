//! «Вакансии»: выбор вакансии, её доска по этапам с перетаскиванием и создание вакансии.
use leptos::prelude::*;
use recruitment_contracts::{ApplicationDto, CreateVacancyRequest, Stage, VacancyDto};
use serde_json::json;
use ui_kit::{EmptyState, Notice, NoticeKind, PageHeader, SessionCtx};

use super::{
    FunnelCtx,
    format::{LONG_OPEN_DAYS, time_label},
    panel::PersonPanel,
    server::{
        AppQuery, Reply, application_action, create_vacancy, list_applications, list_vacancies,
    },
};
use crate::candidates::list::can_write;

/// Одна страница на всю доску вакансии (предел API).
const BOARD_LIMIT: i64 = 200;

fn number(raw: &str) -> Result<Option<i32>, ()> {
    let raw = raw.trim();
    if raw.is_empty() {
        Ok(None)
    } else {
        raw.parse().map(Some).map_err(|_| ())
    }
}

/// Нужно ли развернуть форму создания: адрес `?new=1` и право записи.
pub fn wants_new_form(value: Option<&str>, writable: bool) -> bool {
    writable && matches!(value, Some("1" | "true"))
}

#[component]
pub fn VacanciesPage() -> impl IntoView {
    let session = expect_context::<SessionCtx>();
    let ctx = FunnelCtx::new();
    let writable = Signal::derive(move || can_write(session));
    let selected = RwSignal::new(String::new());
    let creating = RwSignal::new(false);
    // ссылка из шапки и палитры раскрывает форму, в том числе когда экран уже открыт
    let query = leptos_router::hooks::use_query_map();
    Effect::new(move |_| {
        if wants_new_form(query.get().get("new").as_deref(), writable.get()) {
            creating.set(true);
        }
    });

    // список перечитывается только при создании вакансии: доска не должна пересоздаваться
    let list_version = RwSignal::new(0u32);
    let vacancies = Resource::new(move || list_version.get(), |_| list_vacancies());
    Effect::new(move |_| {
        if let Some(Ok(Reply::Ok(list))) = vacancies.get()
            && !list
                .iter()
                .any(|v| v.id.to_string() == selected.get_untracked())
            && let Some(first) = list.first()
        {
            selected.set(first.id.to_string());
        }
    });

    view! {
        <PageHeader title="Вакансии">
            <Show when=move || writable.get()>
                <button class="btn btn--primary" data-testid="new-vacancy" on:click=move |_| creating.update(|c| *c = !*c)>
                    "Новая вакансия"
                </button>
            </Show>
        </PageHeader>

        <Show when=move || creating.get()>
            <VacancyForm list_version=list_version selected=selected creating=creating />
        </Show>

        <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
            {move || {
                vacancies
                    .get()
                    .map(|result| match result {
                        Ok(Reply::Ok(list)) if list.is_empty() => {
                            view! {
                                <EmptyState
                                    title="Вакансий пока нет"
                                    hint="Создайте первую вакансию, и на ней появится доска откликов."
                                />
                            }
                                .into_any()
                        }
                        Ok(Reply::Ok(list)) => {
                            view! { <Board list=list selected=selected ctx=ctx writable=writable /> }.into_any()
                        }
                        _ => {
                            view! {
                                <Notice kind=NoticeKind::Error>
                                    "Не удалось загрузить вакансии. Попробуйте позже."
                                </Notice>
                            }
                                .into_any()
                        }
                    })
            }}
        </Transition>
        <PersonPanel ctx=ctx writable=writable />
    }
}

#[component]
fn VacancyForm(
    list_version: RwSignal<u32>,
    selected: RwSignal<String>,
    creating: RwSignal<bool>,
) -> impl IntoView {
    let title = RwSignal::new(String::new());
    let department = RwSignal::new(String::new());
    let city = RwSignal::new(String::new());
    let openings = RwSignal::new(String::new());
    let priority = RwSignal::new("normal".to_string());
    let manager = RwSignal::new(String::new());
    let recruiter = RwSignal::new(String::new());
    let skills = RwSignal::new(String::new());
    let open_days = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);

    let create = Action::new(move |request: &CreateVacancyRequest| {
        let request = request.clone();
        async move { create_vacancy(request).await }
    });
    Effect::new(move |_| {
        if let Some(result) = create.value().get() {
            match result {
                Ok(Reply::Ok(v)) => {
                    error.set(None);
                    selected.set(v.id.to_string());
                    creating.set(false);
                    list_version.update(|v| *v += 1);
                }
                Ok(other) => error.set(other.failure()),
                Err(_) => error.set(Some("Не удалось создать вакансию.".into())),
            }
        }
    });

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let (Ok(openings), Ok(open_days)) = (number(&openings.get()), number(&open_days.get()))
        else {
            error.set(Some("План найма и срок открытия — числа.".into()));
            return;
        };
        let opt = |s: String| Some(s.trim().to_string()).filter(|s| !s.is_empty());
        create.dispatch(CreateVacancyRequest {
            title: title.get(),
            department: opt(department.get()),
            city: opt(city.get()),
            openings,
            priority: Some(priority.get()),
            hiring_manager: opt(manager.get()),
            recruiter: opt(recruiter.get()),
            required_skills: crate::candidates::format::split_skills(&skills.get()),
            open_days,
        });
    };

    view! {
        <form class="card section" on:submit=submit data-testid="vacancy-form">
            <div class="form-grid">
                <label>
                    "Название"
                    <input type="text" required data-testid="vf-title" on:input=move |ev| title.set(event_target_value(&ev)) />
                </label>
                <label>
                    "Подразделение"
                    <input type="text" on:input=move |ev| department.set(event_target_value(&ev)) />
                </label>
                <label>
                    "Город" <input type="text" on:input=move |ev| city.set(event_target_value(&ev)) />
                </label>
                <label>
                    "План найма"
                    <input type="text" inputmode="numeric" placeholder="1" on:input=move |ev| openings.set(event_target_value(&ev)) />
                </label>
                <label>
                    "Приоритет"
                    <select on:change=move |ev| priority.set(event_target_value(&ev))>
                        <option value="normal" selected>"Обычный"</option>
                        <option value="high">"Высокий"</option>
                        <option value="low">"Низкий"</option>
                    </select>
                </label>
                <label>
                    "Нанимающий менеджер"
                    <input type="text" on:input=move |ev| manager.set(event_target_value(&ev)) />
                </label>
                <label>
                    "Рекрутер"
                    <input type="text" on:input=move |ev| recruiter.set(event_target_value(&ev)) />
                </label>
                <label>
                    "Требуемые навыки через запятую"
                    <input type="text" on:input=move |ev| skills.set(event_target_value(&ev)) />
                </label>
                <label>
                    "Открыта дней"
                    <input type="text" inputmode="numeric" placeholder="0" on:input=move |ev| open_days.set(event_target_value(&ev)) />
                </label>
            </div>
            {move || error.get().map(|e| view! { <Notice kind=NoticeKind::Error>{e}</Notice> })}
            <p>
                <button type="submit" class="btn btn--primary" data-testid="vf-submit" disabled=move || create.pending().get()>
                    "Создать вакансию"
                </button>
            </p>
        </form>
    }
}

#[component]
fn Board(
    list: Vec<VacancyDto>,
    selected: RwSignal<String>,
    ctx: FunnelCtx,
    writable: Signal<bool>,
) -> impl IntoView {
    let options = list.clone();
    let info = move || {
        let id = selected.get();
        list.iter().find(|v| v.id.to_string() == id).cloned()
    };

    // отклики выбранной вакансии; локальная копия нужна для мгновенного переноса карточки
    let apps = Resource::new(
        move || (selected.get(), ctx.version.get()),
        |(vacancy, _)| async move {
            if vacancy.is_empty() {
                return None;
            }
            Some(
                list_applications(AppQuery {
                    vacancy_id: vacancy,
                    page_size: BOARD_LIMIT,
                    ..AppQuery::default()
                })
                .await,
            )
        },
    );
    let items = RwSignal::new(Vec::<ApplicationDto>::new());
    Effect::new(move |_| {
        if let Some(Some(Ok(Reply::Ok(page)))) = apps.get() {
            items.set(page.items);
        }
    });

    let dragging = RwSignal::new(None::<String>);
    let over = RwSignal::new(None::<Stage>);
    let message = RwSignal::new(None::<String>);
    let mover = Action::new(move |(id, stage): &(String, Stage)| {
        let (id, stage) = (id.clone(), *stage);
        async move { application_action(id, "stage".into(), json!({"stage": stage.key()})).await }
    });
    Effect::new(move |_| {
        if let Some(result) = mover.value().get() {
            match result {
                Ok(reply) => match reply.failure() {
                    None => message.set(None),
                    Some(text) => {
                        message.set(Some(text));
                        ctx.bump(); // карточка возвращается в исходную колонку
                    }
                },
                Err(_) => {
                    message.set(Some("Не удалось перевести отклик.".into()));
                    ctx.bump();
                }
            }
        }
    });

    let drop_on = move |stage: Stage| {
        let Some(id) = dragging.get_untracked() else {
            return;
        };
        dragging.set(None);
        over.set(None);
        let mut moved = false;
        items.update(|all| {
            if let Some(a) = all.iter_mut().find(|a| a.id.to_string() == id)
                && a.stage != stage
            {
                a.stage = stage;
                a.hours_on_stage = 0;
                a.overdue = false;
                moved = true;
            }
        });
        if moved {
            mover.dispatch((id, stage));
        }
    };

    view! {
        <div class="vacancy-bar">
            <select
                aria-label="Вакансия"
                data-testid="vacancy-select"
                on:change=move |ev| selected.set(event_target_value(&ev))
                prop:value=move || selected.get()
            >
                {options
                    .into_iter()
                    .map(|v| {
                        let id = v.id.to_string();
                        view! { <option value=id.clone() selected=move || selected.get() == id>{v.title}</option> }
                    })
                    .collect_view()}
            </select>
            {move || {
                info()
                    .map(|v| {
                        let long = v.open_days > LONG_OPEN_DAYS;
                        view! {
                            <div class="facts-inline" data-testid="vacancy-facts">
                                <span>"План найма: " <b>{v.openings}</b></span>
                                <span>
                                    "Открыта: "
                                    <b class:urgent=long data-testid="open-days">{format!("{} д", v.open_days)}</b>
                                </span>
                                {v.hiring_manager.clone().map(|m| view! { <span>"Менеджер: " {m}</span> })}
                                {v.recruiter.clone().map(|m| view! { <span>"Рекрутер: " {m}</span> })}
                                <span data-testid="active-count">
                                    {move || {
                                        format!(
                                            "Активных откликов: {}",
                                            items.get().iter().filter(|a| a.stage.is_active()).count(),
                                        )
                                    }}
                                </span>
                            </div>
                        }
                    })
            }}
        </div>
        {move || message.get().map(|m| view! { <Notice kind=NoticeKind::Error>{m}</Notice> })}
        <Transition fallback=|| view! { <p class="muted">"Загрузка доски…"</p> }>
            {move || {
                apps.get()
                    .flatten()
                    .map(|result| match result {
                        Ok(Reply::Ok(_)) => ().into_any(),
                        _ => {
                            view! {
                                <Notice kind=NoticeKind::Error>
                                    "Не удалось загрузить отклики. Попробуйте позже."
                                </Notice>
                            }
                                .into_any()
                        }
                    })
            }}
        </Transition>
        <div class="board" data-testid="board">
            {Stage::ALL
                .into_iter()
                .map(|stage| {
                    let in_column = move || {
                        items.get().into_iter().filter(|a| a.stage == stage).collect::<Vec<_>>()
                    };
                    view! {
                        <section
                            class="column"
                            class:column--over=move || over.get() == Some(stage)
                            data-stage=stage.key()
                            on:dragover=move |ev| {
                                ev.prevent_default();
                                over.set(Some(stage));
                            }
                            on:dragleave=move |_| {
                                if over.get_untracked() == Some(stage) {
                                    over.set(None);
                                }
                            }
                            on:drop=move |ev| {
                                ev.prevent_default();
                                drop_on(stage);
                            }
                        >
                            <header class="column__head">
                                <span class="column__title">{format!("{} · {}", stage.number(), stage.label())}</span>
                                <span class="column__count" data-testid=format!("count-{}", stage.key())>
                                    {move || in_column().len()}
                                </span>
                            </header>
                            <div class="column__body">
                                <For
                                    each=in_column
                                    key=|a| (a.id, a.stage)
                                    children=move |a| {
                                        let id = a.id.to_string();
                                        let drag_id = id.clone();
                                        let open_id = id.clone();
                                        view! {
                                            <div
                                                class="app-card"
                                                class:app-card--dragging=move || dragging.get().as_deref() == Some(id.as_str())
                                                draggable=move || writable.get().to_string()
                                                data-testid="app-card"
                                                data-app=drag_id.clone()
                                                tabindex="0"
                                                on:dragstart=move |ev| {
                                                    if let Some(dt) = ev.data_transfer() {
                                                        let _ = dt.set_data("text/plain", &drag_id);
                                                    }
                                                    dragging.set(Some(drag_id.clone()));
                                                }
                                                on:dragend=move |_| {
                                                    dragging.set(None);
                                                    over.set(None);
                                                }
                                                on:click=move |_| ctx.open.set(Some(open_id.clone()))
                                            >
                                                <span class="app-card__name">{a.full_name.clone()}</span>
                                                <span class="app-card__meta">
                                                    {a.match_percent.map(|m| view! { <span class="mono">{format!("{m}%")}</span> })}
                                                    {a.source.clone().map(|s| view! { <span>{s}</span> })}
                                                    <span class="mono" class:urgent=a.overdue>{time_label(a.hours_on_stage)}</span>
                                                    {(a.person_applications > 1)
                                                        .then(|| view! { <span class="chip">{format!("{} отклика", a.person_applications)}</span> })}
                                                </span>
                                            </div>
                                        }
                                    }
                                />
                            </div>
                        </section>
                    }
                })
                .collect_view()}
        </div>
    }
}

#[cfg(test)]
mod new_form_tests {
    use super::wants_new_form;

    #[test]
    fn the_form_opens_only_for_new_1_and_a_writer() {
        assert!(wants_new_form(Some("1"), true));
        assert!(wants_new_form(Some("true"), true));
        assert!(!wants_new_form(Some("1"), false));
        assert!(!wants_new_form(Some("0"), true));
        assert!(!wants_new_form(None, true));
    }
}
