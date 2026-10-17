//! Вкладки раздела и панель инструментов.
use leptos::prelude::*;
use leptos_router::hooks::use_location;
use ui_core::tabs::active_tab;

/// Вкладка раздела: адрес страницы, подпись и идентификатор для тестов.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tab {
    pub href: &'static str,
    pub label: &'static str,
    pub testid: &'static str,
}

/// Вкладки раздела. Текущая определяется по адресу страницы (`aria-current="page"`): вложенные
/// страницы относятся к вкладке с самым длинным подходящим адресом.
#[component]
pub fn Tabs(
    items: Vec<Tab>,
    /// Подпись набора вкладок для скринридеров.
    label: &'static str,
    #[prop(optional)] testid: Option<&'static str>,
) -> impl IntoView {
    let location = use_location();
    let hrefs: Vec<&'static str> = items.iter().map(|t| t.href).collect();
    let current = Memo::new(move |_| active_tab(&location.pathname.get(), &hrefs));
    view! {
        <nav class="tabs" aria-label=label data-testid=testid>
            {items
                .into_iter()
                .enumerate()
                .map(|(i, tab)| {
                    view! {
                        <a
                            href=tab.href
                            class="tab"
                            data-testid=tab.testid
                            aria-current=move || (current.get() == Some(i)).then_some("page")
                        >
                            {tab.label}
                        </a>
                    }
                })
                .collect_view()}
        </nav>
    }
}

/// Панель инструментов: кнопки и поля в ряд, на узком экране переносятся.
#[component]
pub fn Toolbar(
    /// Подпись группы для скринридеров.
    #[prop(optional)]
    label: Option<&'static str>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="toolbar" role=label.map(|_| "group") aria-label=label>
            {children()}
        </div>
    }
}
