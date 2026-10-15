//! Клиентское приложение рекрутеров (Leptos, SSR + hydrate). Общается только с bff-crm.
// глубокие вложенные представления доски превышают лимит по умолчанию при сборке wasm в release
#![recursion_limit = "512"]
pub mod candidates;
pub mod funnel;

use leptos::prelude::*;
use leptos_meta::{MetaTags, Title, provide_meta_context};
use leptos_router::{
    SsrMode,
    components::{ParentRoute, Route, Router, Routes},
    path,
};
use ui_kit::{
    AppShell, AuthGate, EmptyState, LoginPage, NavItem, SessionCtx, UiStyles, provide_session,
};

/// HTML-оболочка документа для SSR.
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="ru">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <AutoReload options=options.clone() />
                <HydrationScripts options />
                <MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    provide_session();

    view! {
        <Title text="Ритм" />
        <UiStyles />
        <Router>
            <Routes fallback=|| view! { <EmptyState title="Страница не найдена" hint="Проверьте адрес." /> }>
                <Route
                    path=path!("/login")
                    view=|| {
                        view! {
                            <LoginPage
                                with_tenant=true
                                title="Ритм"
                                subtitle="Войдите в рабочее пространство вашей организации."
                            />
                        }
                    }
                    ssr=SsrMode::Async
                />
                <ParentRoute path=path!("") view=Protected ssr=SsrMode::Async>
                    <Route path=path!("/") view=funnel::today::TodayPage ssr=SsrMode::Async />
                    <Route path=path!("/triage") view=funnel::triage::TriagePage ssr=SsrMode::Async />
                    <Route path=path!("/candidates") view=candidates::list::CandidatesPage ssr=SsrMode::Async />
                    <Route path=path!("/candidates/new") view=candidates::create::CreateCandidatePage ssr=SsrMode::Async />
                    <Route path=path!("/candidates/:id") view=candidates::card::CandidateCardPage ssr=SsrMode::Async />
                    <Route path=path!("/vacancies") view=funnel::vacancies::VacanciesPage ssr=SsrMode::Async />
                    <Route path=path!("/applications") view=funnel::applications::ApplicationsPage ssr=SsrMode::Async />
                </ParentRoute>
            </Routes>
        </Router>
    }
}

fn nav() -> Vec<NavItem> {
    vec![
        NavItem {
            href: "/",
            label: "Сегодня",
        },
        NavItem {
            href: "/vacancies",
            label: "Вакансии",
        },
        NavItem {
            href: "/applications",
            label: "Отклики",
        },
        NavItem {
            href: "/candidates",
            label: "Кандидаты",
        },
    ]
}

/// Защищённая часть: без сессии отправляет на вход, с сессией показывает каркас.
#[component]
fn Protected() -> impl IntoView {
    let ctx = expect_context::<SessionCtx>();
    // T открывает режим разбора (не в полях ввода и не поверх панели человека)
    let navigate = leptos_router::hooks::use_navigate();
    Effect::new(move |_| {
        let navigate = navigate.clone();
        window_event_listener(leptos::ev::keydown, move |ev| {
            if ev.ctrl_key() || ev.meta_key() || ev.alt_key() || ev.key().to_lowercase() != "t" {
                return;
            }
            if !candidates::list::can_write(ctx) || typing_in_a_field(&ev) || panel_is_open() {
                return;
            }
            ev.prevent_default();
            navigate("/triage", Default::default());
        });
    });
    let user = move || {
        ctx.current()
            .and_then(Result::ok)
            .and_then(|s| s.user)
            .unwrap_or_default()
    };
    view! {
        <AuthGate>
            <AppShell
                brand="Ритм"
                nav=nav()
                context_label=Signal::derive(move || user().tenant)
                user_label=Signal::derive(move || user().email)
            />
        </AuthGate>
    }
}

/// Фокус в поле ввода: горячие клавиши не должны перехватывать набор текста.
fn typing_in_a_field(ev: &leptos::ev::KeyboardEvent) -> bool {
    use leptos::wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
        .is_some_and(|el| {
            matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT")
                || el.is_content_editable()
        })
}

fn panel_is_open() -> bool {
    document()
        .query_selector("[data-testid=panel]")
        .ok()
        .flatten()
        .is_some()
}

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
