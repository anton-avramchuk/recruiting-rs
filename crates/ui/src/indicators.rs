//! Индикаторы: сообщение и пустое состояние.
use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Error,
}

#[component]
pub fn Notice(kind: NoticeKind, children: Children) -> impl IntoView {
    let class = match kind {
        NoticeKind::Info => "notice notice--info",
        NoticeKind::Error => "notice notice--error",
    };
    view! { <div class=class role="alert">{children()}</div> }
}

#[component]
pub fn EmptyState(title: &'static str, hint: &'static str) -> impl IntoView {
    view! {
        <div class="empty">
            <h2>{title}</h2>
            <p class="muted">{hint}</p>
        </div>
    }
}

/// Заголовок страницы с местом для действий справа.
#[component]
pub fn PageHeader(
    title: &'static str,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class="page-header">
            <h1>{title}</h1>
            <div class="page-header__actions">{children.map(|c| c())}</div>
        </div>
    }
}

/// Карточка: поверхность с рамкой. `class` добавляет модификаторы (`section`, `list-card`).
#[component]
pub fn Card(#[prop(default = "")] class: &'static str, children: Children) -> impl IntoView {
    view! { <div class=format!("card {class}").trim().to_string()>{children()}</div> }
}

/// Небольшая метка: тег, статус, число.
#[component]
pub fn Chip(#[prop(optional)] testid: Option<&'static str>, children: Children) -> impl IntoView {
    view! {
        <span class="chip" data-testid=testid>
            {children()}
        </span>
    }
}

/// Тон плашки: цвет означает состояние, а не декор.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeTone {
    Ok,
    Warn,
    Muted,
    /// Выключено или заблокировано.
    Off,
}

/// Плашка состояния.
#[component]
pub fn Badge(
    #[prop(default = BadgeTone::Muted)] tone: BadgeTone,
    #[prop(optional)] testid: Option<&'static str>,
    children: Children,
) -> impl IntoView {
    let class = match tone {
        BadgeTone::Ok => "badge badge--ok",
        BadgeTone::Warn => "badge badge--warn",
        BadgeTone::Muted => "badge badge--muted",
        BadgeTone::Off => "badge badge--off",
    };
    view! {
        <span class=class data-testid=testid>
            {children()}
        </span>
    }
}

/// Круг с инициалами человека; скринридеру достаточно соседнего имени.
#[component]
pub fn Avatar(#[prop(into)] name: String) -> impl IntoView {
    view! { <span class="thumb" aria-hidden="true">{ui_core::format::initials(&name)}</span> }
}
