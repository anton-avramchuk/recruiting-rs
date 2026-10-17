//! «Сегодня»: сводка, очередь на разбор, интервью дня и вакансии в работе.
use leptos::prelude::*;
use leptos_router::components::A;
use recruitment_contracts::{ApplicationDto, InterviewItemDto, OverviewDto, Stage, VacancyDto};
use ui_kit::Reply;
use ui_kit::{Avatar, Notice, NoticeKind, PageHeader, SessionCtx};

use super::{
    FunnelCtx,
    format::{LONG_OPEN_DAYS, local_clock, local_day_range, time_label, triage_minutes},
    panel::PersonPanel,
    server::{AppQuery, get_overview, list_applications, list_interviews, list_vacancies},
};
use crate::candidates::{format::format_salary, list::can_write};

/// Сколько откликов очереди показывает страница.
pub const QUEUE_PREVIEW: i64 = 9;
/// Больше стольких новых откликов число выделяется сигнальным цветом.
const BACKLOG_ALERT: i64 = 20;

fn stage_count(o: &OverviewDto, stage: Stage) -> i64 {
    o.by_stage
        .iter()
        .find(|c| c.stage == stage)
        .map_or(0, |c| c.count)
}

#[component]
pub fn TodayPage() -> impl IntoView {
    let session = expect_context::<SessionCtx>();
    let ctx = FunnelCtx::new();
    let writable = Signal::derive(move || can_write(session));

    let overview = Resource::new(move || ctx.version.get(), |_| get_overview());
    let queue = Resource::new(
        move || ctx.version.get(),
        |_| {
            list_applications(AppQuery {
                stage: "new".into(),
                sort: "match".into(),
                desc: true,
                page: 1,
                page_size: QUEUE_PREVIEW,
                ..AppQuery::default()
            })
        },
    );
    let vacancies = Resource::new(move || ctx.version.get(), |_| list_vacancies());

    // сутки пользователя известны только в браузере: интервью запрашиваются после гидратации
    let day = RwSignal::new(None::<(String, String)>);
    Effect::new(move |_| day.set(local_day_range()));
    let interviews = Resource::new(
        move || (day.get(), ctx.version.get()),
        |(day, _)| async move {
            match day {
                Some((from, to)) => Some(list_interviews(from, to).await),
                None => None,
            }
        },
    );

    view! {
        <PageHeader title="Сегодня" />
        <div class="grid3">
            <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
                {move || {
                    overview
                        .get()
                        .map(|result| match result {
                            Ok(Reply::Ok(o)) => {
                                view! { <Stats o=o writable=writable /> }.into_any()
                            }
                            _ => {
                                view! {
                                    <Notice kind=NoticeKind::Error>"Не удалось загрузить сводку. Попробуйте позже."</Notice>
                                }
                                    .into_any()
                            }
                        })
                }}
            </Transition>
        </div>
        <div class="grid2">
            <section>
                <div class="sec-h">
                    <h2>"Очередь на разбор"</h2>
                    <span class="muted">"по совпадению"</span>
                </div>
                <div class="card list-card" data-testid="queue">
                    <Transition fallback=|| view! { <p class="muted pad">"Загрузка…"</p> }>
                        {move || {
                            queue
                                .get()
                                .map(|result| match result {
                                    Ok(Reply::Ok(page)) if page.items.is_empty() => {
                                        view! { <div class="empty-inline"><b>"Очередь пуста"</b>" Все отклики разобраны."</div> }
                                            .into_any()
                                    }
                                    Ok(Reply::Ok(page)) => {
                                        page.items
                                            .into_iter()
                                            .map(|a| view! { <QueueRow a=a ctx=ctx /> })
                                            .collect_view()
                                            .into_any()
                                    }
                                    _ => view! { <p class="muted pad">"Очередь сейчас недоступна."</p> }.into_any(),
                                })
                        }}
                    </Transition>
                </div>
            </section>
            <section>
                <div class="sec-h">
                    <h2>"Интервью сегодня"</h2>
                </div>
                <div class="card list-card" data-testid="interviews">
                    <Transition fallback=|| view! { <p class="muted pad">"Загрузка…"</p> }>
                        {move || {
                            interviews
                                .get()
                                .flatten()
                                .map(|result| match result {
                                    Ok(Reply::Ok(list)) if list.is_empty() => {
                                        view! { <div class="empty-inline">"Интервью не назначены."</div> }.into_any()
                                    }
                                    Ok(Reply::Ok(list)) => {
                                        list.into_iter()
                                            .map(|i| view! { <InterviewRow i=i ctx=ctx /> })
                                            .collect_view()
                                            .into_any()
                                    }
                                    _ => view! { <p class="muted pad">"Интервью сейчас недоступны."</p> }.into_any(),
                                })
                        }}
                    </Transition>
                </div>
                <div class="sec-h">
                    <h2>"Вакансии в работе"</h2>
                    <A href="/vacancies" attr:class="btn btn--ghost">"Все вакансии"</A>
                </div>
                <div class="card list-card" data-testid="vacancy-list">
                    <Transition fallback=|| view! { <p class="muted pad">"Загрузка…"</p> }>
                        {move || {
                            vacancies
                                .get()
                                .map(|result| match result {
                                    Ok(Reply::Ok(list)) if list.is_empty() => {
                                        view! { <div class="empty-inline">"Вакансий пока нет."</div> }.into_any()
                                    }
                                    Ok(Reply::Ok(list)) => {
                                        list.into_iter()
                                            .take(6)
                                            .map(|v| view! { <VacancyRow v=v /> })
                                            .collect_view()
                                            .into_any()
                                    }
                                    _ => ().into_any(),
                                })
                        }}
                    </Transition>
                </div>
            </section>
        </div>
        <PersonPanel ctx=ctx writable=writable />
    }
}

#[component]
fn Stats(o: OverviewDto, writable: Signal<bool>) -> impl IntoView {
    let waiting = o.new_waiting_over_day;
    let minutes = triage_minutes(o.new_total as usize);
    let chips = [
        Stage::Screening,
        Stage::Hr,
        Stage::Tech,
        Stage::Final,
        Stage::Offer,
    ]
    .into_iter()
    .map(|s| {
        view! {
            <span class="chip" title=s.label()>
                {s.number()} <b class="mono">{stage_count(&o, s)}</b>
            </span>
        }
    })
    .collect_view();
    view! {
        <div class="card stat" data-testid="stat-new">
            <div class="lbl">"Новых откликов"</div>
            <div class="v" class:urgent=(o.new_total > BACKLOG_ALERT) data-testid="new-total">
                {o.new_total}
            </div>
            <div class="d" data-testid="new-waiting">{format!("{waiting} ждут дольше суток")}</div>
            <Show when=move || writable.get() && (o.new_total > 0)>
                <A href="/triage" attr:class="btn btn--primary btn--block" attr:data-testid="start-triage">
                    {format!("Разобрать · ≈{minutes} мин")}
                    <kbd>"T"</kbd>
                </A>
            </Show>
        </div>
        <div class="card stat" data-testid="stat-work">
            <div class="lbl">"В работе"</div>
            <div class="v" data-testid="in-work">{o.in_work}</div>
            <div class="d">
                {format!("{} на этапе оффера · {} человек в базе", stage_count(&o, Stage::Offer), o.people)}
            </div>
            <div class="chips">{chips}</div>
        </div>
        <div class="card stat" data-testid="stat-overdue">
            <div class="lbl">"Просрочено по SLA"</div>
            <div class="v" class:urgent=(o.overdue > 0) data-testid="overdue-total">{o.overdue}</div>
            <div class="d">"Без движения дольше нормы этапа"</div>
            <A href="/applications?overdue=1" attr:class="btn btn--block" attr:data-testid="show-overdue">
                "Показать список"
            </A>
        </div>
    }
}

#[component]
fn QueueRow(a: ApplicationDto, ctx: FunnelCtx) -> impl IntoView {
    let id = a.id.to_string();
    let meta = [
        Some(a.vacancy_title.clone()),
        a.source.clone(),
        a.expected_salary.map(format_salary),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    view! {
        <button type="button" class="qrow" data-testid="queue-row" on:click=move |_| ctx.open.set(Some(id.clone()))>
            <Avatar name=a.full_name.to_string() />
            <span class="qrow__main">
                <span class="nm">{a.full_name.clone()}</span>
                <span class="sub">{meta}</span>
            </span>
            <span class="chip">
                {a.match_percent.map(|m| format!("{m}%")).unwrap_or_else(|| "—".into())}
            </span>
            <span class="mono" class:urgent=a.overdue>{time_label(a.hours_on_stage)}</span>
        </button>
    }
}

#[component]
fn InterviewRow(i: InterviewItemDto, ctx: FunnelCtx) -> impl IntoView {
    let id = i.application_id.to_string();
    let sub = [
        Some(i.stage.label().to_string()),
        Some(i.vacancy_title.clone()),
        i.client_name.clone(),
        i.contact_name.clone(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    view! {
        <button type="button" class="qrow" data-testid="interview-row" on:click=move |_| ctx.open.set(Some(id.clone()))>
            <span class="mono time">{local_clock(&i.scheduled_at)}</span>
            <span class="qrow__main">
                <span class="nm">{i.full_name.clone()}</span>
                <span class="sub">{sub}</span>
            </span>
        </button>
    }
}

#[component]
fn VacancyRow(v: VacancyDto) -> impl IntoView {
    view! {
        <A href="/vacancies" attr:class="qrow" attr:data-testid="vacancy-row">
            <span class="qrow__main">
                <span class="nm">{v.title.clone()}</span>
                <span class="sub">
                    {[v.client.as_ref().map(|c| c.name.clone()), v.city.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · ")}
                </span>
            </span>
            <span class="mono">{format!("{} чел", v.active_applications)}</span>
            <span class="mono" class:urgent=(v.open_days > LONG_OPEN_DAYS)>{format!("{} дн", v.open_days)}</span>
        </A>
    }
}
