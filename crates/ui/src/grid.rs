//! Компонент `Grid`.
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use grid_contracts::{GridPage, GridQuery, Operator};
use leptos::prelude::*;
use leptos_router::{
    NavigateOptions,
    components::A,
    hooks::{use_location, use_navigate, use_query_map},
};
use serde::{Serialize, de::DeserializeOwned};

use crate::{ConfirmDialog, EmptyState, Notice, NoticeKind, Pager};
use ui_core::grid::{FilterSpec, GridUrl, PAGE_SIZES, RowActions, is_interactive, page_info};

/// Пауза в наборе перед запросом, мс.
const DEBOUNCE_MS: u64 = 300;

/// Фильтр под заголовком столбца.
#[derive(Clone)]
pub enum ColumnFilter {
    None,
    /// Текст: условие «содержит».
    Text,
    /// Выбор значения из списка (значение, подпись); `Operator` - как сравнивать с полем.
    Select(Signal<Vec<(String, String)>>, Operator),
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

    pub fn filter_select(mut self, options: Signal<Vec<(String, String)>>, op: Operator) -> Self {
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

/// Свои действия строки: рисует содержимое ячейки действий для записи.
pub type RowExtra<T> = Arc<dyn Fn(&T) -> AnyView + Send + Sync>;

/// Удаление записи из строки грида: название для диалога подтверждения, разрешение на запись и
/// асинхронное удаление; текст ошибки показывается в диалоге.
pub struct RowRemove<T> {
    label: Arc<dyn Fn(&T) -> String + Send + Sync>,
    allowed: Arc<dyn Fn(&T) -> bool + Send + Sync>,
    run: Arc<dyn Fn(T) -> RemoveFuture + Send + Sync>,
}

type RemoveFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

impl<T> Clone for RowRemove<T> {
    fn clone(&self) -> Self {
        Self {
            label: self.label.clone(),
            allowed: self.allowed.clone(),
            run: self.run.clone(),
        }
    }
}

impl<T: 'static> RowRemove<T> {
    /// Удалять можно любую запись; `label` - как назвать запись в вопросе.
    pub fn new<F, Fut>(label: fn(&T) -> String, run: F) -> Self
    where
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        Self {
            label: Arc::new(label),
            allowed: Arc::new(|_| true),
            run: Arc::new(move |item| Box::pin(run(item))),
        }
    }

    /// Ограничивает записи, у которых показывается «Удалить».
    pub fn allowed_when(mut self, allowed: fn(&T) -> bool) -> Self {
        self.allowed = Arc::new(allowed);
        self
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
    /// Страница записи: по ней ведут двойной щелчок по строке и действие «Изменить».
    #[prop(default = None)]
    row_href: Option<fn(&T) -> String>,
    /// Удаление записи из строки (с подтверждением); без него действия «Удалить» нет.
    #[prop(default = None)]
    remove: Option<RowRemove<T>>,
    /// Свои действия строки: показываются в столбце действий рядом со стандартными.
    #[prop(default = None)]
    extra_actions: Option<RowExtra<T>>,
    /// Внешний счётчик перезагрузок: при изменении страница читается заново.
    #[prop(default = None)]
    refresh: Option<Signal<u32>>,
    /// Поля, по которым работает общий поиск; пусто - поиска нет.
    #[prop(optional)]
    search_fields: Vec<&'static str>,
    #[prop(default = "Поиск")] search_placeholder: &'static str,
    #[prop(default = "Пока ничего нет")] empty_title: &'static str,
    #[prop(default = "Записи появятся здесь.")] empty_hint: &'static str,
    /// Кнопки справа от поиска.
    #[prop(default = None)]
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
    let has_actions = row_href.is_some() || remove.is_some() || extra_actions.is_some();
    let extra_actions = StoredValue::new(extra_actions);
    let width = columns.len() + usize::from(has_actions);
    let remove = StoredValue::new(remove);
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

    // открытие записи двойным щелчком: переход делает эффект (навигатор не Send)
    let open_href = RwSignal::new(None::<String>);
    let open_navigate = use_navigate();
    Effect::new(move |_| {
        if let Some(href) = open_href.get() {
            open_navigate(&href, Default::default());
        }
    });

    // удаление из строки: диалог подтверждения, затем перечитывание страницы
    let version = RwSignal::new(0u32);
    let confirm_open = RwSignal::new(false);
    let target = RwSignal::new(None::<T>);
    let remove_error = RwSignal::new(None::<String>);
    let removing = Action::new(move |item: &T| {
        let item = item.clone();
        let run = remove.with_value(|r| r.as_ref().map(|r| r.run.clone()));
        async move {
            match run {
                Some(run) => run(item).await,
                None => Ok(()),
            }
        }
    });
    Effect::new(move |_| match removing.value().get() {
        Some(Ok(())) => {
            confirm_open.set(false);
            target.set(None);
            version.update(|v| *v += 1);
        }
        Some(Err(message)) => remove_error.set(Some(message)),
        None => {}
    });
    let confirm_label = Signal::derive(move || {
        target
            .get()
            .zip(remove.with_value(|r| r.clone()))
            .map(|(item, r)| (r.label)(&item))
            .unwrap_or_default()
    });

    let rows = Resource::new(
        move || {
            let current = url.get();
            let q = specs.with_value(|specs| {
                search_fields.with_value(|fields| current.to_grid_query(specs, fields))
            });
            (q, version.get(), refresh.map_or(0, |r| r.get()))
        },
        move |(q, _, _)| load(q),
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
    let actions_head = has_actions
        .then(|| view! { <th scope="col" class="grid__actions-head"><span class="muted">"Действия"</span></th> });

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
                            let options = *options;
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
                                    {move || {
                                        options
                                            .get()
                                            .into_iter()
                                            .map(|(value, label)| {
                                                let current = value.clone();
                                                view! {
                                                    <option value=value selected=move || url.get().filters.get(field).is_some_and(|v| *v == current)>
                                                        {label}
                                                    </option>
                                                }
                                            })
                                            .collect_view()
                                    }}
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
                                    .map(|col| view! { <td>{(col.cell)(&item)}</td> })
                                    .collect_view()
                            });
                            let href = row_href.map(|href| href(&item));
                            let can_remove = remove.with_value(|r| {
                                r.as_ref().map(|r| (r.allowed)(&item))
                            });
                            let actions = RowActions::new(
                                href.is_some(),
                                can_remove.is_some(),
                                can_remove.unwrap_or(false),
                            );
                            let actions_cell = has_actions.then(|| {
                                let edit = actions.edit.then(|| {
                                    view! {
                                        <A href=href.clone().unwrap_or_default() attr:class="btn" attr:data-testid="row-edit">
                                            "Изменить"
                                        </A>
                                    }
                                });
                                let removal = actions.remove.then(|| {
                                    let item = item.clone();
                                    view! {
                                        <button
                                            type="button"
                                            class="btn"
                                            data-testid="row-remove"
                                            on:click=move |_| {
                                                remove_error.set(None);
                                                target.set(Some(item.clone()));
                                                confirm_open.set(true);
                                            }
                                        >
                                            "Удалить"
                                        </button>
                                    }
                                });
                                let extra = extra_actions.with_value(|e| e.as_ref().map(|f| f(&item)));
                                view! { <td class="grid__actions-cell"><span class="grid__actions">{extra}{edit}{removal}</span></td> }
                            });
                            view! {
                                <tr
                                    data-testid="grid-row"
                                    on:dblclick=move |ev| {
                                        use leptos::wasm_bindgen::JsCast;
                                        let inner = ev
                                            .target()
                                            .and_then(|t| t.dyn_into::<leptos::web_sys::Element>().ok())
                                            .is_some_and(|e| is_interactive(&e.tag_name()));
                                        if !inner && let Some(href) = &href {
                                            open_href.set(Some(href.clone()));
                                        }
                                    }
                                >
                                    {cells}
                                    {actions_cell}
                                </tr>
                            }
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
                Ok(p) => (
                    page_info(p.page, p.page_size, p.items.len(), p.total),
                    p.pages(),
                    p.page,
                    p.page_size,
                ),
                Err(_) => (String::new(), 1, 1, url.get_untracked().page_size),
            };
            view! {
                <Pager
                    page=page
                    pages=pages
                    caption=info
                    on_page=Callback::new(move |p: u32| change(&|u| u.with_page(p)))
                    size=size
                    on_size=Callback::new(move |s: u32| change(&|u| u.with_page_size(s)))
                    sizes=&PAGE_SIZES
                    testid="grid"
                />
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
                        <tr>{head}{actions_head}</tr>
                        {has_filters
                            .then(|| view! { <tr class="grid__filters">{filter_row}{has_actions.then(|| view! { <td></td> })}</tr> })}
                    </thead>
                    <Transition fallback=move || view! { <tbody><tr><td colspan=width.to_string() class="muted">"Загрузка…"</td></tr></tbody> }>
                        {body}
                    </Transition>
                </table>
            </div>
            <Transition fallback=|| ()>{pager}</Transition>
            <ConfirmDialog
                open=confirm_open
                label=confirm_label
                busy=Signal::derive(move || removing.pending().get())
                error=Signal::derive(move || remove_error.get())
                on_confirm=Callback::new(move |()| {
                    remove_error.set(None);
                    if let Some(item) = target.get_untracked() {
                        removing.dispatch(item);
                    }
                })
            />
        </div>
    }
}
