//! Компонент `Grid`.
use std::{future::Future, sync::Arc, time::Duration};

use grid_contracts::{GridPage, GridQuery, Operator};
use leptos::prelude::*;
use leptos_router::{
    NavigateOptions,
    components::A,
    hooks::{use_location, use_navigate, use_query_map},
};
use serde::{Serialize, de::DeserializeOwned};

use super::state::{FilterSpec, GridUrl, PAGE_SIZES, page_info};
use crate::{EmptyState, Notice, NoticeKind};

/// Пауза в наборе перед запросом, мс.
const DEBOUNCE_MS: u64 = 300;

/// Фильтр под заголовком столбца.
#[derive(Clone)]
pub enum ColumnFilter {
    None,
    /// Текст: условие «содержит».
    Text,
    /// Выбор значения из списка (значение, подпись); `Operator` - как сравнивать с полем.
    Select(Vec<(&'static str, &'static str)>, Operator),
}

/// Столбец таблицы.
pub struct Column<T> {
    /// Имя поля грида: по нему сортируют и фильтруют.
    pub field: &'static str,
    pub header: &'static str,
    pub sortable: bool,
    pub filter: ColumnFilter,
    pub cell: Arc<dyn Fn(&T) -> AnyView + Send + Sync>,
}

impl<T> Clone for Column<T> {
    fn clone(&self) -> Self {
        Self {
            field: self.field,
            header: self.header,
            sortable: self.sortable,
            filter: self.filter.clone(),
            cell: self.cell.clone(),
        }
    }
}

impl<T> Column<T> {
    pub fn new(
        field: &'static str,
        header: &'static str,
        cell: impl Fn(&T) -> AnyView + Send + Sync + 'static,
    ) -> Self {
        Self {
            field,
            header,
            sortable: false,
            filter: ColumnFilter::None,
            cell: Arc::new(cell),
        }
    }

    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }

    pub fn filter_text(mut self) -> Self {
        self.filter = ColumnFilter::Text;
        self
    }

    pub fn filter_select(
        mut self,
        options: Vec<(&'static str, &'static str)>,
        op: Operator,
    ) -> Self {
        self.filter = ColumnFilter::Select(options, op);
        self
    }

    fn filter_spec(&self) -> Option<FilterSpec> {
        match &self.filter {
            ColumnFilter::None => None,
            ColumnFilter::Text => Some(FilterSpec {
                field: self.field,
                op: Operator::Contains,
            }),
            ColumnFilter::Select(_, op) => Some(FilterSpec {
                field: self.field,
                op: *op,
            }),
        }
    }
}

/// Текстовое поле: введённое применяется после паузы в наборе; внешнее изменение значения (сброс,
/// переход по ссылке) заменяет текст, если пользователь сейчас не печатает.
#[component]
pub fn DebouncedText(
    value: Signal<String>,
    on_apply: Callback<String>,
    #[prop(into)] placeholder: String,
    #[prop(into)] label: String,
    testid: String,
) -> impl IntoView {
    let text = RwSignal::new(value.get_untracked());
    let typing = RwSignal::new(false);
    let generation = RwSignal::new(0u32);
    Effect::new(move |_| {
        let external = value.get();
        if !typing.get_untracked() && text.get_untracked() != external {
            text.set(external);
        }
    });
    view! {
        <input
            type="search"
            placeholder=placeholder
            aria-label=label
            data-testid=testid
            prop:value=move || text.get()
            on:input=move |ev| {
                let typed = event_target_value(&ev);
                text.set(typed.clone());
                typing.set(true);
                let current = generation.get_untracked() + 1;
                generation.set(current);
                set_timeout(
                    move || {
                        if generation.try_get_untracked() == Some(current) {
                            typing.set(false);
                            on_apply.run(typed);
                        }
                    },
                    Duration::from_millis(DEBOUNCE_MS),
                );
            }
        />
    }
}

/// Таблица с серверными страницами.
///
/// `load` получает готовый запрос и возвращает страницу либо текст ошибки; состояние (страница, сортировка,
/// поиск, фильтры столбцов) читается из адресной строки и пишется в неё без новых записей в истории.
#[component]
pub fn Grid<T, F, Fut>(
    columns: Vec<Column<T>>,
    load: F,
    /// Ссылка строки: первый столбец оборачивается в неё.
    #[prop(optional)]
    row_href: Option<fn(&T) -> String>,
    /// Поля, по которым работает общий поиск; пусто - поиска нет.
    #[prop(optional)]
    search_fields: Vec<&'static str>,
    #[prop(default = "Поиск")] search_placeholder: &'static str,
    #[prop(default = "Пока ничего нет")] empty_title: &'static str,
    #[prop(default = "Записи появятся здесь.")] empty_hint: &'static str,
    /// Кнопки справа от поиска.
    #[prop(optional)]
    toolbar: Option<ViewFn>,
) -> impl IntoView
where
    T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
    F: Fn(GridQuery) -> Fut + Copy + Send + Sync + 'static,
    Fut: Future<Output = Result<GridPage<T>, String>> + Send + 'static,
{
    let filter_fields: Vec<&'static str> = columns
        .iter()
        .filter_map(|c| c.filter_spec())
        .map(|s| s.field)
        .collect();
    let specs: Vec<FilterSpec> = columns.iter().filter_map(|c| c.filter_spec()).collect();
    let searchable = !search_fields.is_empty();
    let has_filters = !filter_fields.is_empty();
    let width = columns.len();
    let columns = StoredValue::new(columns);
    let specs = StoredValue::new(specs);
    let search_fields = StoredValue::new(search_fields);
    let filter_fields = StoredValue::new(filter_fields);

    let query = use_query_map();
    let location = use_location();
    let navigate = use_navigate();
    let url = Memo::new(move |_| {
        let params = query.get();
        filter_fields.with_value(|fields| GridUrl::from_lookup(|key| params.get(key), fields))
    });
    // интерфейс только просит новый вид, переход делает один эффект (браузерный навигатор не Send)
    let pending = RwSignal::new(None::<GridUrl>);
    Effect::new(move |_| {
        if let Some(next) = pending.get() {
            let qs = next.to_query_string();
            let path = location.pathname.get_untracked();
            let target = if qs.is_empty() {
                path
            } else {
                format!("{path}?{qs}")
            };
            navigate(
                &target,
                NavigateOptions {
                    replace: true,
                    scroll: false,
                    ..Default::default()
                },
            );
        }
    });
    let change = move |f: &dyn Fn(GridUrl) -> GridUrl| pending.set(Some(f(url.get_untracked())));

    let rows = Resource::new(
        move || {
            let current = url.get();
            specs.with_value(|specs| {
                search_fields.with_value(|fields| current.to_grid_query(specs, fields))
            })
        },
        load,
    );

    let search = view! {
        <DebouncedText
            value=Signal::derive(move || url.get().q)
            on_apply=Callback::new(move |v: String| change(&|u| u.with_q(&v)))
            placeholder=search_placeholder
            label=search_placeholder
            testid="grid-search".to_string()
        />
    };
    let reset = move || {
        view! {
            <Show when=move || url.get().is_filtered()>
                <button type="button" class="btn btn--ghost" data-testid="grid-reset" on:click=move |_| change(&|u| u.reset())>
                    "Сбросить"
                </button>
            </Show>
        }
    };

    let head = move || {
        columns.with_value(|cols| {
            cols.iter()
                .map(|col| {
                    let field = col.field;
                    let header = col.header;
                    let label = if col.sortable {
                        view! {
                            <button
                                type="button"
                                class="grid__sort"
                                data-testid=format!("grid-sort-{field}")
                                on:click=move |_| change(&|u| u.toggle_sort(field))
                            >
                                {header}
                                <span aria-hidden="true">
                                    {move || match url.get().sort {
                                        Some(s) if s.field == field && s.desc => " ▼",
                                        Some(s) if s.field == field => " ▲",
                                        _ => "",
                                    }}
                                </span>
                            </button>
                        }
                        .into_any()
                    } else {
                        header.into_any()
                    };
                    view! {
                        <th
                            scope="col"
                            aria-sort=move || match url.get().sort {
                                Some(s) if s.field == field && s.desc => "descending",
                                Some(s) if s.field == field => "ascending",
                                _ => "none",
                            }
                        >
                            {label}
                        </th>
                    }
                })
                .collect_view()
        })
    };

    let filter_row = move || {
        columns.with_value(|cols| {
            cols.iter()
                .map(|col| {
                    let field = col.field;
                    let control = match &col.filter {
                        ColumnFilter::None => ().into_any(),
                        ColumnFilter::Text => view! {
                            <DebouncedText
                                value=Signal::derive(move || url.get().filters.get(field).cloned().unwrap_or_default())
                                on_apply=Callback::new(move |v: String| change(&|u| u.with_filter(field, &v)))
                                placeholder="Фильтр"
                                label=format!("Фильтр: {}", col.header)
                                testid=format!("grid-filter-{field}")
                            />
                        }
                        .into_any(),
                        ColumnFilter::Select(options, _) => {
                            let options = options.clone();
                            view! {
                                <select
                                    aria-label=format!("Фильтр: {}", col.header)
                                    data-testid=format!("grid-filter-{field}")
                                    on:change=move |ev| {
                                        let value = event_target_value(&ev);
                                        change(&|u| u.with_filter(field, &value));
                                    }
                                >
                                    <option value="" selected=move || !url.get().filters.contains_key(field)>"Все"</option>
                                    {options
                                        .into_iter()
                                        .map(|(value, label)| {
                                            view! {
                                                <option value=value selected=move || url.get().filters.get(field).is_some_and(|v| v == value)>
                                                    {label}
                                                </option>
                                            }
                                        })
                                        .collect_view()}
                                </select>
                            }
                            .into_any()
                        }
                    };
                    view! { <td>{control}</td> }
                })
                .collect_view()
        })
    };

    let body = move || {
        rows.get().map(|result| match result {
            Err(message) => view! {
                <tbody>
                    <tr>
                        <td colspan=width.to_string() data-testid="grid-error">
                            <Notice kind=NoticeKind::Error>{message}</Notice>
                        </td>
                    </tr>
                </tbody>
            }
            .into_any(),
            Ok(page) if page.items.is_empty() => {
                let filtered = url.get_untracked().has_conditions();
                view! {
                    <tbody>
                        <tr>
                            <td colspan=width.to_string() data-testid="grid-empty">
                                {if filtered {
                                    view! { <EmptyState title="Ничего не найдено" hint="Измените условия поиска или сбросьте фильтры." /> }
                                        .into_any()
                                } else {
                                    view! { <EmptyState title=empty_title hint=empty_hint /> }.into_any()
                                }}
                            </td>
                        </tr>
                    </tbody>
                }
                .into_any()
            }
            Ok(page) => view! {
                <tbody>
                    {page
                        .items
                        .into_iter()
                        .map(|item| {
                            let cells = columns.with_value(|cols| {
                                cols.iter()
                                    .enumerate()
                                    .map(|(i, col)| {
                                        let content = (col.cell)(&item);
                                        match (i, row_href) {
                                            (0, Some(href)) => {
                                                view! { <td><A href=href(&item)>{content}</A></td> }.into_any()
                                            }
                                            _ => view! { <td>{content}</td> }.into_any(),
                                        }
                                    })
                                    .collect_view()
                            });
                            view! { <tr data-testid="grid-row">{cells}</tr> }
                        })
                        .collect_view()}
                </tbody>
            }
            .into_any(),
        })
    };

    let pager = move || {
        rows.get().map(|result| {
            let (info, pages, page, size) = match &result {
                Ok(p) => (page_info(p.page, p.page_size, p.items.len(), p.total), p.pages(), p.page, p.page_size),
                Err(_) => (String::new(), 1, 1, url.get_untracked().page_size),
            };
            view! {
                <div class="grid__pager" data-testid="grid-pager">
                    <span class="muted" data-testid="grid-page-info">{info}</span>
                    <span class="spacer"></span>
                    <label class="grid__size">
                        "Строк"
                        <select
                            data-testid="grid-page-size"
                            on:change=move |ev| {
                                let size: u32 = event_target_value(&ev).parse().unwrap_or(20);
                                change(&|u| u.with_page_size(size));
                            }
                        >
                            {PAGE_SIZES
                                .into_iter()
                                .map(|s| view! { <option value=s.to_string() selected=move || s == size>{s.to_string()}</option> })
                                .collect_view()}
                        </select>
                    </label>
                    <button
                        type="button"
                        class="btn"
                        data-testid="grid-prev"
                        disabled={page <= 1}
                        on:click=move |_| change(&|u| { let p = u.page.saturating_sub(1); u.with_page(p) })
                    >
                        "Назад"
                    </button>
                    <button
                        type="button"
                        class="btn"
                        data-testid="grid-next"
                        disabled={page >= pages}
                        on:click=move |_| change(&|u| { let p = u.page + 1; u.with_page(p) })
                    >
                        "Вперёд"
                    </button>
                </div>
            }
        })
    };

    view! {
        <div class="grid" data-testid="grid">
            <div class="toolbar">
                {searchable.then_some(search)}
                {toolbar.map(|t| t.run())}
                {reset}
            </div>
            <div class="card grid__card">
                <table class="grid__table">
                    <thead>
                        <tr>{head}</tr>
                        {has_filters.then(|| view! { <tr class="grid__filters">{filter_row}</tr> })}
                    </thead>
                    <Transition fallback=move || view! { <tbody><tr><td colspan=width.to_string() class="muted">"Загрузка…"</td></tr></tbody> }>
                        {body}
                    </Transition>
                </table>
            </div>
            <Transition fallback=|| ()>{pager}</Transition>
        </div>
    }
}
