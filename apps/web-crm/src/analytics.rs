//! «Аналитика»: воронка, сроки, источники, динамика откликов и нагрузка на вакансии.
use leptos::prelude::*;
use leptos_router::components::A;
use recruitment_contracts::{AnalyticsDto, ClientStatDto, FunnelStepDto, Stage};
use ui_kit::Reply;
use ui_kit::{Card, Chip, Dropdown, Notice, NoticeKind, PageHeader, Toolbar};

use crate::funnel::{format::LONG_OPEN_DAYS, server::get_analytics};

/// Периоды переключателя, суток.
pub const PERIODS: [i64; 4] = [30, 90, 180, 365];

/// Доля `value` от `max` в процентах для ширины полосы (0…100).
pub fn bar_percent(value: i64, max: i64) -> f64 {
    if max <= 0 {
        0.0
    } else {
        (value as f64 / max as f64 * 100.0).clamp(0.0, 100.0)
    }
}

/// Число с запятой вместо точки; целое — без дробной части: `20` -> «20», `2.5` -> «2,5».
pub fn number(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}").replace('.', ",")
    }
}

/// Значение показателя или прочерк, если данных нет.
pub fn metric(value: Option<f64>, unit: &str) -> String {
    value.map_or("—".to_string(), |v| format!("{} {unit}", number(v)))
}

/// «02.03» из «2026-03-02».
pub fn week_label(week_start: &str) -> String {
    match (week_start.get(8..10), week_start.get(5..7)) {
        (Some(day), Some(month)) => format!("{day}.{month}"),
        _ => week_start.to_string(),
    }
}

/// «30 дней», «365 дней».
pub fn period_label(days: i64) -> String {
    format!("{days} дней")
}

/// «Срок найма» заказчика: медиана в сутках или прочерк.
pub fn client_hire_time(c: &ClientStatDto) -> String {
    metric(c.median_days_to_hire, "дн")
}

/// Нет ли в организации откликов за период (воронка пуста).
pub fn is_empty(a: &AnalyticsDto) -> bool {
    a.funnel.first().is_none_or(|s| s.reached == 0)
}

#[component]
pub fn AnalyticsPage() -> impl IntoView {
    let days = RwSignal::new(90i64);
    // выбранный заказчик: пустая строка - вся организация
    let client = RwSignal::new(String::new());
    let report = Resource::new(
        move || (days.get(), client.get()),
        |(days, client)| get_analytics(days, client),
    );

    view! {
        <PageHeader title="Аналитика">
            <Toolbar label="Период">
                {PERIODS
                    .into_iter()
                    .map(|p| {
                        view! {
                            <button
                                class="btn"
                                class:btn--primary=move || days.get() == p
                                data-testid=format!("period-{p}")
                                aria-pressed=move || (days.get() == p).to_string()
                                on:click=move |_| days.set(p)
                            >
                                {period_label(p)}
                            </button>
                        }
                    })
                    .collect_view()}
            </Toolbar>
        </PageHeader>
        <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
            {move || {
                report
                    .get()
                    .map(|result| match result {
                        Ok(Reply::Ok(a)) => view! { <Report a=a client=client /> }.into_any(),
                        _ => {
                            view! {
                                <Notice kind=NoticeKind::Error>"Не удалось загрузить аналитику. Попробуйте позже."</Notice>
                            }
                                .into_any()
                        }
                    })
            }}
        </Transition>
    }
}

#[component]
fn Report(a: AnalyticsDto, client: RwSignal<String>) -> impl IntoView {
    let empty = is_empty(&a);
    let options = a
        .clients
        .iter()
        .filter_map(|c| c.client_id.map(|id| (id.to_string(), c.name.clone())))
        .collect::<Vec<_>>();
    let client_rows = client_rows(&a.clients, client);
    let max = a.funnel.iter().map(|s| s.reached).max().unwrap_or(0);
    let weekly_max = a.weekly.iter().map(|w| w.count).max().unwrap_or(0);
    let worst = a.worst_stage;
    let last_week = a.weekly.len().saturating_sub(1);

    let funnel = a
        .funnel
        .iter()
        .map(|s| funnel_row(s, max, worst == Some(s.stage)))
        .collect_view();
    let weeks = a
        .weekly
        .iter()
        .enumerate()
        .map(|(i, w)| {
            view! {
                <div
                    class="b"
                    class:b--now=(i == last_week)
                    style=format!("height:{:.0}%", bar_percent(w.count, weekly_max).max(2.0))
                    title=format!("{}: {} откликов", week_label(&w.week_start), w.count)
                    data-testid="week-bar"
                    data-count=w.count.to_string()
                ></div>
            }
        })
        .collect_view();
    let sources = a
        .sources
        .iter()
        .map(|s| {
            view! {
                <tr data-testid="source-row">
                    <td>{s.source.clone()}</td>
                    <td class="mono">{s.total}</td>
                    <td class="mono">{s.interviews}</td>
                    <td class="mono">{format!("{}%", s.conversion)}</td>
                </tr>
            }
        })
        .collect_view();
    let vacancies = a
        .vacancies
        .iter()
        .map(|v| {
            let long = v.open_days > LONG_OPEN_DAYS;
            view! {
                <div class="qrow" data-testid="load-row">
                    <span class="qrow__main">
                        <span class="nm">{v.title.clone()}</span>
                        <span class="sub">{v.recruiter.clone().unwrap_or_else(|| "рекрутер не назначен".into())}</span>
                    </span>
                    <span class="mono">{format!("{} в работе", v.active_applications)}</span>
                    <span class="mono" class:urgent=long>{format!("{} дн", v.open_days)}</span>
                </div>
            }
        })
        .collect_view();

    view! {
        <Toolbar>
            <label>
                "Заказчик "
                <Dropdown
                    value=client
                    options=Signal::stored(options)
                    empty="Все заказчики"
                    testid="client-filter"
                    aria_label="Заказчик"
                />
            </label>
        </Toolbar>
        {empty
            .then(|| {
                view! {
                    <p class="muted" data-testid="analytics-empty">
                        "Данных пока нет: метрики появятся, когда в организации будут отклики."
                    </p>
                }
            })}
        <div class="grid3">
            <div class="card stat" data-testid="stat-hire">
                <div class="lbl">"Срок найма, медиана"</div>
                <div class="v">{metric(a.median_days_to_hire, "дн")}</div>
                <div class="d">"От отклика до выхода"</div>
            </div>
            <div class="card stat" data-testid="stat-touch">
                <div class="lbl">"Отклик → первое касание"</div>
                <div class="v">{metric(a.median_hours_to_first_touch, "ч")}</div>
                <div class="d">"Норма по SLA — 24 ч"</div>
            </div>
            <div class="card stat" data-testid="stat-repeat">
                <div class="lbl">"Повторные кандидаты"</div>
                <div class="v">{a.repeat_people}</div>
                <div class="d">"Откликались больше одного раза"</div>
            </div>
        </div>
        <div class="grid2">
            <section>
                <div class="sec-h">
                    <h2>"Воронка найма"</h2>
                    <span class="muted">"выделен этап с наибольшей потерей"</span>
                </div>
                <div class="card" data-testid="funnel">{funnel}</div>
                <div class="sec-h">
                    <h2>"Отклики по неделям"</h2>
                </div>
                <Card>
                    <div class="bars" data-testid="weekly">{weeks}</div>
                    <div class="legend">
                        <span>{a.weekly.first().map(|w| week_label(&w.week_start))}</span>
                        <span class="spacer"></span>
                        <span>{a.weekly.last().map(|w| week_label(&w.week_start))}</span>
                    </div>
                </Card>
            </section>
            <section>
                <div class="sec-h">
                    <h2>"Источники"</h2>
                    <span class="muted">"по конверсии в интервью"</span>
                </div>
                <Card class="list-card">
                    <table data-testid="sources">
                        <thead>
                            <tr>
                                <th>"Источник"</th>
                                <th>"Всего"</th>
                                <th>"В интервью"</th>
                                <th>"Конверсия"</th>
                            </tr>
                        </thead>
                        <tbody>{sources}</tbody>
                    </table>
                </Card>
                <div class="sec-h">
                    <h2>"Заказчики"</h2>
                    <span class="muted">"по числу откликов"</span>
                </div>
                {client_rows}
                <div class="sec-h">
                    <h2>"Нагрузка на вакансии"</h2>
                </div>
                <div class="card list-card" data-testid="load">{vacancies}</div>
            </section>
        </div>
    }
}

/// Таблица заказчиков: клик по названию включает фильтр, рядом ссылка на карточку.
fn client_rows(clients: &[ClientStatDto], selected: RwSignal<String>) -> impl IntoView + use<> {
    if clients.is_empty() {
        return view! {
            <p class="muted" data-testid="clients-empty">"Заказчиков с вакансиями пока нет."</p>
        }
        .into_any();
    }
    let rows = clients
        .iter()
        .map(|c| {
            let id = c.client_id.map(|id| id.to_string());
            let name = c.name.clone();
            let filter_id = id.clone();
            let compare_id = id.clone();
            view! {
                <tr
                    data-testid="client-stat-row"
                    class:row-selected=move || compare_id.as_ref().is_some_and(|i| *i == selected.get())
                >
                    <td>
                        {match filter_id {
                            Some(id) => {
                                view! {
                                    <button
                                        class="btn btn--ghost"
                                        data-testid="client-filter-row"
                                        on:click=move |_| selected.set(id.clone())
                                    >
                                        {name.clone()}
                                    </button>
                                }
                                    .into_any()
                            }
                            None => view! { <span data-testid="client-none">{name.clone()}</span> }.into_any(),
                        }}
                        {c.archived.then(|| view! { <Chip>"архив"</Chip> })}
                        {id
                            .map(|id| {
                                view! {
                                    <A href=format!("/clients/{id}") attr:data-testid="client-card-link" attr:aria-label="Карточка заказчика">
                                        "↗"
                                    </A>
                                }
                            })}
                    </td>
                    <td class="mono">{c.vacancies}</td>
                    <td class="mono" data-testid="client-applications">{c.applications}</td>
                    <td class="mono">{format!("{}%", c.conversion)}</td>
                    <td class="mono">{c.hired}</td>
                    <td class="mono" data-testid="client-hire-time">{client_hire_time(c)}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <Card class="list-card">
            <table data-testid="clients-table">
                <thead>
                    <tr>
                        <th>"Заказчик"</th>
                        <th>"Вакансий"</th>
                        <th>"Откликов"</th>
                        <th>"В интервью"</th>
                        <th>"Вышли"</th>
                        <th>"Срок найма"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </Card>
    }
    .into_any()
}

fn funnel_row(step: &FunnelStepDto, max: i64, worst: bool) -> impl IntoView + use<> {
    let label = format!("{} · {}", step.stage.number(), step.stage.label());
    let width = bar_percent(step.reached, max);
    let stage: Stage = step.stage;
    view! {
        <div class="funrow" class:funrow--worst=worst data-testid="funnel-row" data-stage=stage.key()>
            <span class="funrow__label">{label}</span>
            <span class="funbar"><i style=format!("width:{width:.0}%")></i></span>
            <span class="mono" data-testid="funnel-count">{step.reached}</span>
            <span class="mono muted">{format!("{}%", step.conversion)}</span>
            {worst.then(|| view! { <Chip testid="worst">"наибольшая потеря"</Chip> })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_are_proportional_and_clamped() {
        assert_eq!(bar_percent(5, 10), 50.0);
        assert_eq!(bar_percent(0, 10), 0.0);
        assert_eq!(bar_percent(7, 0), 0.0);
        assert_eq!(bar_percent(20, 10), 100.0);
        assert_eq!(bar_percent(-1, 10), 0.0);
    }

    #[test]
    fn numbers_use_a_comma_and_drop_a_zero_fraction() {
        assert_eq!(number(20.0), "20");
        assert_eq!(number(2.5), "2,5");
        assert_eq!(number(4.04), "4");
        assert_eq!(number(4.26), "4,3");
        assert_eq!(number(0.0), "0");
    }

    #[test]
    fn a_missing_metric_is_a_dash() {
        assert_eq!(metric(None, "дн"), "—");
        assert_eq!(metric(Some(31.0), "дн"), "31 дн");
        assert_eq!(metric(Some(4.5), "ч"), "4,5 ч");
    }

    #[test]
    fn week_labels_are_day_and_month() {
        assert_eq!(week_label("2026-03-02"), "02.03");
        assert_eq!(week_label("мусор"), "мусор");
    }

    #[test]
    fn a_missing_hire_time_is_a_dash() {
        let mut stat = ClientStatDto {
            client_id: None,
            name: "Acme".into(),
            archived: false,
            vacancies: 1,
            active_applications: 0,
            applications: 0,
            interviews: 0,
            conversion: 0,
            hired: 0,
            median_days_to_hire: None,
        };
        assert_eq!(client_hire_time(&stat), "—");
        stat.median_days_to_hire = Some(20.0);
        assert_eq!(client_hire_time(&stat), "20 дн");
    }

    #[test]
    fn period_labels_name_the_days() {
        assert_eq!(period_label(30), "30 дней");
        assert_eq!(PERIODS, [30, 90, 180, 365]);
    }
}
