//! Режим разбора: новые отклики по одному, три действия с клавиатуры.
use std::collections::HashMap;

use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_navigate};
use recruitment_contracts::ApplicationDto;
use serde_json::json;
use ui_kit::{Notice, NoticeKind, SessionCtx};

use super::{
    format::{initials, now_ms, time_label},
    server::{AppQuery, Reply, TriageData, application_action, list_applications, triage_card},
};
use crate::candidates::{
    format::{format_salary, years_label},
    list::can_write,
    outcome::LoadOutcome,
};

/// Не больше стольких откликов за один сеанс.
const SESSION_LIMIT: i64 = 200;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Reject,
    Screen,
    Defer,
}

/// Темп разбора: секунд на обработанный отклик.
fn pace(elapsed_ms: f64, done: usize) -> Option<f64> {
    (done > 0).then(|| elapsed_ms / 1000.0 / done as f64)
}

#[component]
pub fn TriagePage() -> impl IntoView {
    let session = expect_context::<SessionCtx>();
    let writable = Signal::derive(move || can_write(session));

    // очередь снимается один раз при входе: пришедшее за время разбора попадёт в следующий сеанс
    let loaded = Resource::new(
        || (),
        |_| {
            list_applications(AppQuery {
                stage: "new".into(),
                sort: "match".into(),
                desc: true,
                page: 1,
                page_size: SESSION_LIMIT,
                ..AppQuery::default()
            })
        },
    );
    let queue = RwSignal::new(Vec::<ApplicationDto>::new());
    let ready = RwSignal::new(false);
    let started = RwSignal::new(0.0f64);
    let elapsed = RwSignal::new(0.0f64);
    let index = RwSignal::new(0usize);
    let done = RwSignal::new(0usize);
    let busy = RwSignal::new(false);
    let message = RwSignal::new(None::<String>);
    let cards = RwSignal::new(HashMap::<usize, TriageData>::new());

    Effect::new(move |_| {
        if let Some(Ok(Reply::Ok(page))) = loaded.get() {
            queue.set(page.items);
            started.set(now_ms());
            ready.set(true);
        }
    });

    // таймер сеанса
    Effect::new(move |_| {
        if let Ok(handle) = set_interval_with_handle(
            move || {
                if ready.get_untracked() && index.get_untracked() < queue.get_untracked().len() {
                    elapsed.set(now_ms() - started.get_untracked());
                }
            },
            std::time::Duration::from_millis(200),
        ) {
            on_cleanup(move || handle.clear());
        }
    });

    // карточка текущего и следующего отклика подгружается заранее
    Effect::new(move |_| {
        if !ready.get() {
            return;
        }
        let at = index.get();
        for i in [at, at + 1] {
            let Some(a) = queue.with_untracked(|q| q.get(i).cloned()) else {
                continue;
            };
            if cards.with_untracked(|c| c.contains_key(&i)) {
                continue;
            }
            leptos::task::spawn_local(async move {
                if let Ok(card) = triage_card(a.id.to_string(), a.person_id.to_string()).await {
                    cards.update(|c| {
                        c.insert(i, card);
                    });
                }
            });
        }
    });

    let act = move |verdict: Verdict| {
        if !writable.get_untracked() || busy.get_untracked() {
            return;
        }
        let at = index.get_untracked();
        let Some(app) = queue.with_untracked(|q| q.get(at).cloned()) else {
            return;
        };
        if verdict == Verdict::Defer {
            message.set(None);
            index.set(at + 1);
            return;
        }
        busy.set(true);
        leptos::task::spawn_local(async move {
            let id = app.id.to_string();
            let result = match verdict {
                Verdict::Reject => application_action(id, "reject".into(), json!({})).await,
                _ => application_action(id, "stage".into(), json!({"stage": "screening"})).await,
            };
            busy.set(false);
            match result {
                Ok(Reply::Ok(_)) => {
                    message.set(None);
                    done.update(|n| *n += 1);
                    index.set(at + 1);
                }
                Ok(other) => message.set(other.failure()),
                Err(_) => message.set(Some(
                    "Не удалось выполнить действие. Попробуйте ещё раз.".into(),
                )),
            }
        });
    };

    // клавиши: ← отказ, → в скрининг, ↓ отложить, Esc выход
    let navigate = use_navigate();
    Effect::new(move |_| {
        let navigate = navigate.clone();
        window_event_listener(leptos::ev::keydown, move |ev| {
            if ev.ctrl_key() || ev.meta_key() || ev.alt_key() {
                return;
            }
            match ev.key().as_str() {
                "ArrowLeft" => {
                    ev.prevent_default();
                    act(Verdict::Reject);
                }
                "ArrowRight" => {
                    ev.prevent_default();
                    act(Verdict::Screen);
                }
                "ArrowDown" => {
                    ev.prevent_default();
                    act(Verdict::Defer);
                }
                "Escape" => navigate("/", Default::default()),
                _ => {}
            }
        });
    });

    let total = move || queue.with(Vec::len);
    let finished = move || ready.get() && index.get() >= total();

    view! {
        <div class="triage" data-testid="triage">
            <div class="triage__head">
                <span class="lbl">"Разбор откликов"</span>
                <span class="mono" data-testid="triage-position">
                    {move || format!("{} / {}", (index.get() + 1).min(total()), total())}
                </span>
                <div class="triage__prog">
                    <i style=move || {
                        let share = if total() == 0 { 0.0 } else { index.get() as f64 / total() as f64 * 100.0 };
                        format!("width:{share}%")
                    }></i>
                </div>
                <span class="mono" data-testid="triage-timer">{move || format!("{:.1} с", elapsed.get() / 1000.0)}</span>
                <A href="/" attr:class="btn" attr:data-testid="triage-exit">
                    "Выйти" <kbd>"Esc"</kbd>
                </A>
            </div>
            <div class="triage__body">
                <Transition fallback=|| view! { <p class="muted">"Загрузка очереди…"</p> }>
                {move || {
                    if !writable.get() {
                        return view! {
                            <div class="triage__done" data-testid="triage-forbidden">
                                <h2>"Разбор недоступен"</h2>
                                <p class="muted">"Для разбора нужно право на изменение откликов."</p>
                            </div>
                        }
                            .into_any();
                    }
                    if !ready.get() {
                        return match loaded.get() {
                            Some(Ok(Reply::Ok(_))) | None => view! { <p class="muted">"Загрузка очереди…"</p> }.into_any(),
                            Some(_) => {
                                view! {
                                    <Notice kind=NoticeKind::Error>"Не удалось загрузить очередь. Попробуйте позже."</Notice>
                                }
                                    .into_any()
                            }
                        };
                    }
                    if total() == 0 {
                        return view! {
                            <div class="triage__done" data-testid="triage-empty">
                                <h2>"Очередь пуста"</h2>
                                <p class="muted">"Новых откликов нет."</p>
                                <A href="/" attr:class="btn btn--primary">"Вернуться к работе"</A>
                            </div>
                        }
                            .into_any();
                    }
                    if finished() {
                        let n = done.get_untracked();
                        let secs = elapsed.get_untracked() / 1000.0;
                        return view! {
                            <div class="triage__done" data-testid="triage-summary">
                                <span class="lbl">"Очередь разобрана"</span>
                                <div class="big">{format!("{secs:.0} с")}</div>
                                <p class="muted">
                                    {format!("Разобрано откликов: {n}")}
                                    {pace(elapsed.get_untracked(), n).map(|p| format!(" · {p:.1} с на отклик"))}
                                </p>
                                <A href="/" attr:class="btn btn--primary" attr:data-testid="triage-back">
                                    "Вернуться к работе" <kbd>"Esc"</kbd>
                                </A>
                            </div>
                        }
                            .into_any();
                    }
                    let at = index.get();
                    let Some(app) = queue.with(|q| q.get(at).cloned()) else {
                        return ().into_any();
                    };
                    let app_id = app.id.to_string();
                    view! {
                        <div class="triage__card" data-testid="triage-card" data-app=app_id>
                            <CardBody app=app card=Signal::derive(move || cards.with(|c| c.get(&at).cloned())) />
                        </div>
                    }
                        .into_any()
                }}
                </Transition>
            </div>
            <div class="triage__foot">
                <button
                    class="triage__act"
                    data-testid="triage-reject"
                    disabled=move || busy.get() || finished() || !ready.get() || !writable.get()
                    on:click=move |_| act(Verdict::Reject)
                >
                    <kbd>"←"</kbd>
                    "Отказ с письмом"
                </button>
                <button
                    class="triage__act"
                    data-testid="triage-defer"
                    disabled=move || busy.get() || finished() || !ready.get() || !writable.get()
                    on:click=move |_| act(Verdict::Defer)
                >
                    <kbd>"↓"</kbd>
                    "Отложить"
                </button>
                <button
                    class="triage__act triage__act--go"
                    data-testid="triage-screen"
                    disabled=move || busy.get() || finished() || !ready.get() || !writable.get()
                    on:click=move |_| act(Verdict::Screen)
                >
                    <kbd>"→"</kbd>
                    "В скрининг"
                </button>
                {move || {
                    message
                        .get()
                        .map(|m| view! { <span class="field-error" role="alert" data-testid="triage-error">{m}</span> })
                }}
                <span class="spacer"></span>
                <span class="lbl">
                    "Темп: "
                    <b class="mono" data-testid="triage-pace">
                        {move || {
                            pace(elapsed.get(), done.get()).map_or("—".to_string(), |p| format!("{p:.1}"))
                        }}
                    </b>
                    " с/отклик"
                </span>
            </div>
        </div>
    }
}

#[component]
fn CardBody(app: ApplicationDto, card: Signal<Option<TriageData>>) -> impl IntoView {
    let others = move || {
        card.get()
            .and_then(|c| c.detail.ok())
            .map(|d| {
                d.person_applications
                    .into_iter()
                    .filter(|a| a.id != app.id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let person = move || match card.get().map(|c| c.person) {
        Some(LoadOutcome::Found(p)) => Some(*p),
        _ => None,
    };
    let app_for_header = app.clone();

    view! {
        <div class="triage__top">
            <span class="thumb">{initials(&app.full_name)}</span>
            <div>
                <h2 data-testid="triage-name">{app.full_name.clone()}</h2>
                <p class="muted">
                    {move || {
                        let p = person();
                        [
                            Some(app_for_header.vacancy_title.clone()),
                            app_for_header.city.clone(),
                            p.as_ref().map(|p| years_label(p.experience_years)),
                            app_for_header.expected_salary.map(format_salary),
                        ]
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>()
                            .join(" · ")
                    }}
                </p>
                <div class="chips">
                    {app.source.clone().map(|s| view! { <span class="chip">{s}</span> })}
                    <span class="chip" class:urgent=app.overdue>
                        {format!("{} в очереди", time_label(app.hours_on_stage))}
                    </span>
                </div>
            </div>
            <div class="triage__match">
                <span class="lbl">"совпадение"</span>
                <b>{app.match_percent.map(|m| format!("{m}%")).unwrap_or_else(|| "—".into())}</b>
            </div>
        </div>
        {move || {
            let others = others();
            (!others.is_empty())
                .then(|| {
                    view! {
                        <div class="triage__sec" data-testid="seen-before">
                            <h4>"Мы его уже видели"</h4>
                            {others
                                .into_iter()
                                .map(|o| {
                                    view! {
                                        <p class="sub">
                                            <span class="chip">{o.stage.label()}</span>
                                            " " {o.vacancy_title.clone()} " · " {time_label(o.hours_on_stage)}
                                        </p>
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                })
        }}
        {move || match person() {
            Some(p) => {
                let jobs = p
                    .employments
                    .iter()
                    .take(3)
                    .map(|e| {
                        let period = format!(
                            "{} — {}",
                            e.start,
                            e.end.clone().unwrap_or_else(|| "по настоящее время".into()),
                        );
                        view! {
                            <li>
                                <b>{e.position.clone()}</b>
                                ", "
                                {e.company.clone()}
                                <span class="muted">{format!(" · {period}")}</span>
                            </li>
                        }
                    })
                    .collect_view();
                let education = p
                    .educations
                    .first()
                    .map(|e| {
                        format!(
                            "Образование: {}{}",
                            e.institution,
                            e.specialty.clone().map(|s| format!(", {s}")).unwrap_or_default(),
                        )
                    });
                view! {
                    <div class="triage__sec">
                        <h4>"Где работал"</h4>
                        <ul class="history">{jobs}</ul>
                        {education.map(|t| view! { <p class="muted">{t}</p> })}
                    </div>
                    <div class="triage__sec">
                        <h4>"Почему подходит"</h4>
                        <p>{p.summary.clone().unwrap_or_else(|| "Краткое описание не заполнено.".into())}</p>
                        <div class="chips">
                            {p.skills.iter().map(|s| view! { <span class="chip">{s.clone()}</span> }).collect_view()}
                        </div>
                    </div>
                    {p.note
                        .clone()
                        .map(|n| {
                            view! {
                                <div class="triage__sec">
                                    <h4>"Заметка"</h4>
                                    <p>{n}</p>
                                </div>
                            }
                        })}
                }
                    .into_any()
            }
            None => view! { <p class="muted triage__sec">"Загружаем профиль…"</p> }.into_any(),
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::pace;

    #[test]
    fn pace_is_seconds_per_processed_response() {
        assert_eq!(pace(10_000.0, 0), None);
        assert_eq!(pace(10_000.0, 4), Some(2.5));
    }
}
