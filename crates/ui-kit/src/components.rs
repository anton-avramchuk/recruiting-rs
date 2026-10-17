use leptos::prelude::*;
use leptos_router::components::{A, Outlet};

use crate::session::{SessionCtx, logout};

/// Стили общих контролов. Цвет несёт смысл только в статусах и ошибках.
const CSS: &str = ui::CSS;

#[component]
pub fn UiStyles() -> impl IntoView {
    // скрипт идёт первым в теле: тема применяется до отрисовки остальной разметки
    view! {
        <script inner_html=crate::theme::EARLY_SCRIPT></script>
        <style>{CSS}</style>
    }
}

/// Кнопка переключения темы; состояние читается из корня документа при нажатии.
#[component]
pub fn ThemeButton() -> impl IntoView {
    let dark = RwSignal::new(false);
    view! {
        <button
            type="button"
            class="btn btn--ghost"
            aria-label="Переключить тему"
            title="Переключить тему"
            aria-pressed=move || dark.get().to_string()
            data-testid="theme-button"
            on:click=move |_| {
                dark.set(crate::theme::toggle_theme() == Some("dark"));
            }
        >
            "◐"
        </button>
    }
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
    /// Кнопка поиска в шапке (палитра команд); без неё не показывается.
    #[prop(optional)]
    on_search: Option<Callback<()>>,
    /// Кнопки быстрых действий справа в шапке; без них шапка остаётся прежней.
    #[prop(optional)]
    actions: Option<ViewFn>,
    /// Дополнительные пункты меню (например, показываемые только по роли), после основных.
    #[prop(optional)]
    nav_extra: Option<ViewFn>,
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
                {on_search
                    .map(|cb| {
                        view! {
                            <button
                                type="button"
                                class="shell__search"
                                aria-label="Поиск и команды"
                                data-testid="search-button"
                                on:click=move |_| cb.run(())
                            >
                                <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true">
                                    <circle cx="7" cy="7" r="4.5" />
                                    <path d="M10.5 10.5 14 14" />
                                </svg>
                                <span class="shell__search-text">"Найти человека, вакансию или команду…"</span>
                                <kbd>"Ctrl + K"</kbd>
                            </button>
                        }
                    })}
                <span class="spacer"></span>
                {actions.map(|a| a.run())}
                <span class="shell__user" data-testid="user-label">{move || user_label.get()}</span>
                <ThemeButton />
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
                {nav_extra.map(|extra| extra.run())}
            </nav>
            <main class="shell__main">
                <Outlet />
            </main>
        </div>
    }
}
