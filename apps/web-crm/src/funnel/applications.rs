//! «Отклики»: таблица со сквозным поиском, фильтрами, сортировкой и групповыми действиями.
use leptos::prelude::*;
use recruitment_contracts::{ApplicationDto, Stage};
use ui_kit::{EmptyState, Notice, NoticeKind, PageHeader, SessionCtx};

use super::{
    FunnelCtx,
    format::time_label,
    panel::PersonPanel,
    server::{AppQuery, Reply, bulk_applications, list_applications, list_vacancies},
};
use crate::candidates::{
    format::format_salary,
    list::{DebouncedInput, can_write},
};

const PAGE_SIZE: i64 = 50;

#[component]
pub fn ApplicationsPage() -> impl IntoView {
    let session = expect_context::<SessionCtx>();
    let ctx = FunnelCtx::new();
    let writable = Signal::derive(move || can_write(session));

    let (q, set_q) = signal(String::new());
    let (source, set_source) = signal(String::new());
    let vacancy = RwSignal::new(String::new());
    let stage = RwSignal::new(String::new());
    // «Только просроченные» можно включить ссылкой со страницы «Сегодня»: `?overdue=1`
    let overdue = RwSignal::new(
        leptos_router::hooks::use_query_map()
            .get_untracked()
            .get("overdue")
            .is_some_and(|v| v == "1" || v == "true"),
    );
    let sort = RwSignal::new(String::new());
    let desc = RwSignal::new(false);
    let (page, set_page) = signal(1i64);
    // меняется при сбросе фильтров: поля ввода пересоздаются пустыми
    let epoch = RwSignal::new(0u32);
    let selected = RwSignal::new(Vec::<String>::new());
    let notice = RwSignal::new(None::<(bool, String)>);

    let vacancies = Resource::new(|| (), |_| list_vacancies());
    let results = Resource::new(
        move || {
            (
                q.get(),
                source.get(),
                vacancy.get(),
                stage.get(),
                sort.get(),
                desc.get(),
                page.get(),
                overdue.get(),
                ctx.version.get(),
            )
        },
        |(q, source, vacancy_id, stage, sort, desc, page, overdue, _)| {
            list_applications(AppQuery {
                overdue,
                vacancy_id,
                stage,
                source,
                q,
                sort,
                desc,
                page,
                page_size: PAGE_SIZE,
            })
        },
    );
    let filtered = move || {
        !q.get().trim().is_empty()
            || !source.get().trim().is_empty()
            || !vacancy.get().is_empty()
            || !stage.get().is_empty()
            || overdue.get()
    };

    let bulk = Action::new(
        move |(action, ids, reason): &(String, Vec<String>, String)| {
            let (action, ids, reason) = (action.clone(), ids.clone(), reason.clone());
            async move { bulk_applications(action, ids, reason).await }
        },
    );
    Effect::new(move |_| {
        if let Some(result) = bulk.value().get() {
            match result {
                Ok(Reply::Ok(r)) => {
                    notice.set(Some((
                        true,
                        format!("Изменено: {}, пропущено: {}", r.changed, r.skipped),
                    )));
                    selected.set(Vec::new());
                    ctx.bump();
                }
                Ok(other) => notice.set(Some((false, other.failure().unwrap_or_default()))),
                Err(_) => notice.set(Some((false, "Не удалось выполнить действие.".into()))),
            }
        }
    });
    let run_bulk = move |action: &'static str| {
        let ids = selected.get_untracked();
        if !ids.is_empty() {
            bulk.dispatch((action.to_string(), ids, String::new()));
        }
    };

    let sort_by = move |key: &'static str| {
        move |_| {
            if sort.get_untracked() == key {
                desc.update(|d| *d = !*d);
            } else {
                sort.set(key.to_string());
                desc.set(key == "match");
            }
            set_page.set(1);
        }
    };

    view! {
        <PageHeader title="Отклики" />

        {move || {
            epoch.get();
            view! {
                <div class="toolbar">
                    <DebouncedInput
                        applied=set_q
                        reset_page=set_page
                        placeholder="Поиск по имени, вакансии, городу, навыкам"
                        label="Поиск"
                        testid="search"
                    />
                    <DebouncedInput
                        applied=set_source
                        reset_page=set_page
                        placeholder="Источник"
                        label="Источник"
                        testid="filter-source"
                    />
                </div>
            }
        }}
        <div class="toolbar">
            <select
                aria-label="Вакансия"
                data-testid="filter-vacancy"
                prop:value=move || vacancy.get()
                on:change=move |ev| {
                    vacancy.set(event_target_value(&ev));
                    set_page.set(1);
                }
            >
                <option value="">"Все вакансии"</option>
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
            <select
                aria-label="Этап"
                data-testid="filter-stage"
                prop:value=move || stage.get()
                on:change=move |ev| {
                    stage.set(event_target_value(&ev));
                    set_page.set(1);
                }
            >
                <option value="">"Все этапы"</option>
                {Stage::ALL
                    .into_iter()
                    .map(|s| view! { <option value=s.key()>{s.label()}</option> })
                    .collect_view()}
            </select>
            <label class="check">
                <input
                    type="checkbox"
                    data-testid="filter-overdue"
                    prop:checked=move || overdue.get()
                    on:change=move |ev| {
                        overdue.set(event_target_checked(&ev));
                        set_page.set(1);
                    }
                />
                "Только просроченные"
            </label>
            <Show when=filtered>
                <button
                    class="btn btn--ghost"
                    data-testid="reset-filters"
                    on:click=move |_| {
                        set_q.set(String::new());
                        set_source.set(String::new());
                        vacancy.set(String::new());
                        stage.set(String::new());
                        overdue.set(false);
                        set_page.set(1);
                        epoch.update(|e| *e += 1);
                    }
                >
                    "Сбросить фильтры"
                </button>
            </Show>
        </div>

        {move || {
            notice
                .get()
                .map(|(ok, text)| {
                    let kind = if ok { NoticeKind::Info } else { NoticeKind::Error };
                    view! {
                        <div data-testid="bulk-result">
                            <Notice kind=kind>{text}</Notice>
                        </div>
                    }
                })
        }}

        <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
            {move || {
                results
                    .get()
                    .map(|result| match result {
                        Ok(Reply::Ok(data)) if data.items.is_empty() => {
                            if filtered() {
                                view! {
                                    <EmptyState title="Ничего не найдено" hint="Измените запрос или сбросьте фильтры." />
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <EmptyState
                                        title="Откликов пока нет"
                                        hint="Откликов нет: добавьте кандидата на вакансию."
                                    />
                                }
                                    .into_any()
                            }
                        }
                        Ok(Reply::Ok(data)) => {
                            let pages = ((data.total + data.page_size - 1) / data.page_size).max(1);
                            let page_ids: Vec<String> = data.items.iter().map(|a| a.id.to_string()).collect();
                            let all_selected = {
                                let page_ids = page_ids.clone();
                                move || {
                                    let sel = selected.get();
                                    !page_ids.is_empty() && page_ids.iter().all(|id| sel.contains(id))
                                }
                            };
                            let toggle_all = {
                                let page_ids = page_ids.clone();
                                move |_| {
                                    let all = {
                                        let sel = selected.get_untracked();
                                        page_ids.iter().all(|id| sel.contains(id))
                                    };
                                    selected
                                        .update(|sel| {
                                            sel.retain(|id| !page_ids.contains(id));
                                            if !all {
                                                sel.extend(page_ids.iter().cloned());
                                            }
                                        });
                                }
                            };
                            view! {
                                <p class="muted" data-testid="total">{format!("Найдено: {}", data.total)}</p>
                                <div class="table-wrap">
                                    <table data-testid="applications">
                                        <thead>
                                            <tr>
                                                <Show when=move || writable.get()>
                                                    <th>
                                                        <input
                                                            type="checkbox"
                                                            aria-label="Выбрать все"
                                                            data-testid="select-all"
                                                            prop:checked=all_selected.clone()
                                                            on:change=toggle_all.clone()
                                                        />
                                                    </th>
                                                </Show>
                                                <th>
                                                    <button class="sort" data-testid="sort-name" on:click=sort_by("name")>"Человек"</button>
                                                </th>
                                                <th>
                                                    <button class="sort" data-testid="sort-stage" on:click=sort_by("stage")>"Этап"</button>
                                                </th>
                                                <th>"Вакансия"</th>
                                                <th>
                                                    <button class="sort" data-testid="sort-match" on:click=sort_by("match")>"Совпадение"</button>
                                                </th>
                                                <th>
                                                    <button class="sort" data-testid="sort-salary" on:click=sort_by("salary")>"Ожидания"</button>
                                                </th>
                                                <th>"Источник"</th>
                                                <th>"Город"</th>
                                                <th>
                                                    <button class="sort" data-testid="sort-age" on:click=sort_by("age")>"Движение"</button>
                                                </th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {data
                                                .items
                                                .into_iter()
                                                .map(|a| row(a, selected, ctx, writable))
                                                .collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                                <div class="toolbar">
                                    <span class="muted">{format!("Страница {} из {}", data.page, pages)}</span>
                                    <button class="btn" disabled=move || page.get() <= 1 on:click=move |_| set_page.update(|p| *p -= 1)>
                                        "Назад"
                                    </button>
                                    <button class="btn" disabled=move || page.get() >= pages on:click=move |_| set_page.update(|p| *p += 1)>
                                        "Вперёд"
                                    </button>
                                </div>
                            }
                                .into_any()
                        }
                        _ => {
                            view! {
                                <Notice kind=NoticeKind::Error>"Не удалось загрузить отклики. Попробуйте позже."</Notice>
                            }
                                .into_any()
                        }
                    })
            }}
        </Transition>

        <Show when=move || writable.get() && !selected.get().is_empty()>
            <div class="bulkbar" data-testid="bulkbar">
                <b data-testid="selected-count">{move || format!("Выбрано: {}", selected.get().len())}</b>
                <button class="btn btn--primary" data-testid="bulk-next" disabled=move || bulk.pending().get() on:click=move |_| run_bulk("next")>
                    "Дальше по воронке"
                </button>
                <button class="btn" data-testid="bulk-reject" disabled=move || bulk.pending().get() on:click=move |_| run_bulk("reject")>
                    "Отказ с письмом"
                </button>
                <button class="btn" data-testid="bulk-letter" disabled=move || bulk.pending().get() on:click=move |_| run_bulk("letter")>
                    "Написать всем"
                </button>
                <button class="btn btn--ghost" data-testid="bulk-clear" on:click=move |_| selected.set(Vec::new())>
                    "Снять"
                </button>
            </div>
        </Show>
        <PersonPanel ctx=ctx writable=writable />
    }
}

fn row(
    a: ApplicationDto,
    selected: RwSignal<Vec<String>>,
    ctx: FunnelCtx,
    writable: Signal<bool>,
) -> impl IntoView {
    let id = a.id.to_string();
    let check_id = id.clone();
    let class_id = id.clone();
    let open_id = id;
    view! {
        <tr class:row-selected=move || selected.get().contains(&class_id) data-testid="app-row">
            <Show when=move || writable.get()>
                <td>
                    <input
                        type="checkbox"
                        aria-label="Выбрать"
                        data-testid="select-row"
                        prop:checked={
                            let id = check_id.clone();
                            move || selected.get().contains(&id)
                        }
                        on:change={
                            let id = check_id.clone();
                            move |_| {
                                selected
                                    .update(|sel| {
                                        if let Some(pos) = sel.iter().position(|s| *s == id) {
                                            sel.remove(pos);
                                        } else {
                                            sel.push(id.clone());
                                        }
                                    });
                            }
                        }
                    />
                </td>
            </Show>
            <td>
                <button class="btn btn--ghost" data-testid="open-app" on:click=move |_| ctx.open.set(Some(open_id.clone()))>
                    {a.full_name.clone()}
                </button>
                {(a.person_applications > 1)
                    .then(|| view! { <span class="chip">{format!("{} отклика", a.person_applications)}</span> })}
            </td>
            <td data-testid="row-stage">{a.stage.label()}</td>
            <td>{a.vacancy_title.clone()}</td>
            <td class="mono">{a.match_percent.map(|m| format!("{m}%")).unwrap_or_else(|| "—".into())}</td>
            <td>{a.expected_salary.map(format_salary).unwrap_or_default()}</td>
            <td>{a.source.clone().unwrap_or_default()}</td>
            <td>{a.city.clone().unwrap_or_default()}</td>
            <td class="mono" class:urgent=a.overdue>{time_label(a.hours_on_stage)}</td>
        </tr>
    }
}
