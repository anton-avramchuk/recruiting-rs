use leptos::prelude::*;
use leptos_router::components::{A, Outlet};

use crate::session::{SessionCtx, logout};

/// Стили приложения. Цвет несёт смысл только в статусах и ошибках.
const CSS: &str = include_str!("../style/ui-kit.css");

#[component]
pub fn UiStyles() -> impl IntoView {
    view! { <style>{CSS}</style> }
}

/// Мета-тег с публичным адресом BFF: по нему браузер находит BFF, а пакет остаётся
/// одинаковым для всех окружений. Размещается в `<head>` оболочки документа.
#[component]
pub fn BffMeta() -> impl IntoView {
    let url = use_context::<crate::BffPublicUrl>()
        .map(|u| u.0)
        .or_else(|| use_context::<crate::BffUrl>().map(|u| u.0))
        .unwrap_or_default();
    view! { <meta name="bff-url" content=url /> }
}

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

/// Плашка статуса: цвет означает состояние, а не декор.
#[component]
pub fn StatusBadge(#[prop(into)] status: String) -> impl IntoView {
    let (class, label) = match status.as_str() {
        "active" => ("badge badge--ok", "Активна"),
        "suspended" => ("badge badge--warn", "Приостановлена"),
        "provisioning" => ("badge badge--muted", "Создаётся"),
        _ => ("badge badge--muted", "—"),
    };
    view! { <span class=class>{label}</span> }
}

#[derive(Clone, PartialEq, Eq)]
pub struct NavItem {
    pub href: &'static str,
    pub label: &'static str,
}

/// Каркас приложения: шапка (организация, пользователь, выход), навигация, область содержимого.
#[component]
pub fn AppShell(
    brand: &'static str,
    nav: Vec<NavItem>,
    /// Название организации/раздела справа от бренда.
    #[prop(into)]
    context_label: Signal<String>,
    #[prop(into)] user_label: Signal<String>,
) -> impl IntoView {
    let ctx = expect_context::<SessionCtx>();
    let logout_action = Action::new(|_: &()| async { logout().await });
    let navigate = leptos_router::hooks::use_navigate();
    Effect::new(move |_| {
        if let Some(Ok(())) = logout_action.value().get() {
            ctx.refresh();
            navigate("/login", Default::default());
        }
    });

    view! {
        <div class="shell">
            <header class="shell__bar">
                <span class="brand">{brand}</span>
                <span class="shell__context" data-testid="context-label">{move || context_label.get()}</span>
                <span class="spacer"></span>
                <span class="shell__user" data-testid="user-label">{move || user_label.get()}</span>
                <form
                    method="post"
                    onsubmit="return false"
                    on:submit=move |ev| {
                        ev.prevent_default();
                        logout_action.dispatch(());
                    }
                >
                    <button type="submit" class="btn btn--ghost">"Выйти"</button>
                </form>
            </header>
            <nav class="shell__nav" aria-label="Разделы">
                {nav
                    .into_iter()
                    .map(|item| view! { <A href=item.href>{item.label}</A> })
                    .collect_view()}
            </nav>
            <main class="shell__main">
                <Outlet />
            </main>
        </div>
    }
}
