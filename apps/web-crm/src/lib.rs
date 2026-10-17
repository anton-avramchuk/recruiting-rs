//! Клиентское приложение рекрутеров (Leptos, SSR + hydrate). Общается только с bff-crm.
// глубокие вложенные представления доски превышают лимит по умолчанию при сборке wasm в release
#![recursion_limit = "1024"]
pub mod access;
pub mod analytics;
pub mod candidates;
pub mod clients;
pub mod comments;
pub mod funnel;
pub mod palette;
pub mod security;

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
                <ui_kit::BffMeta />
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
                    <Route path=path!("/candidates/import") view=candidates::import::ImportPage ssr=SsrMode::Async />
                    <Route path=path!("/candidates/new") view=candidates::create::CreateCandidatePage ssr=SsrMode::Async />
                    <Route path=path!("/candidates/:id") view=candidates::card::CandidateCardPage ssr=SsrMode::Async />
                    <Route path=path!("/vacancies") view=funnel::vacancies::VacanciesPage ssr=SsrMode::Async />
                    <Route path=path!("/clients") view=clients::list::ClientsPage ssr=SsrMode::Async />
                    <Route path=path!("/clients/positions") view=clients::positions::PositionsPage ssr=SsrMode::Async />
                    <Route path=path!("/clients/:id") view=clients::card::ClientCardPage ssr=SsrMode::Async />
                    <Route path=path!("/analytics") view=analytics::AnalyticsPage ssr=SsrMode::Async />
                    <Route path=path!("/security") view=security::view::SecurityPage ssr=SsrMode::Async />
                    <Route path=path!("/security/users/new") view=security::user::NewUserPage ssr=SsrMode::Async />
                    <Route path=path!("/security/users/:id") view=security::user::UserPage ssr=SsrMode::Async />
                    <Route path=path!("/security/roles") view=security::roles::RolesPage ssr=SsrMode::Async />
                    <Route path=path!("/security/roles/new") view=security::roles::NewRolePage ssr=SsrMode::Async />
                    <Route path=path!("/security/roles/:id") view=security::roles::RolePage ssr=SsrMode::Async />
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
            href: "/clients",
            label: "Заказчики",
        },
        NavItem {
            href: "/applications",
            label: "Отклики",
        },
        NavItem {
            href: "/candidates",
            label: "Кандидаты",
        },
        NavItem {
            href: "/analytics",
            label: "Аналитика",
        },
    ]
}

/// Защищённая часть: без сессии отправляет на вход, с сессией показывает каркас.
#[component]
fn Protected() -> impl IntoView {
    let ctx = expect_context::<SessionCtx>();
    // клавиатура оболочки: палитра, переходы, разбор; права записи проверяются на месте
    let palette = palette::view::PaletteCtx::new();
    let writable = Signal::derive(move || candidates::list::can_write(ctx));
    provide_context(palette);
    palette::view::install_keys(palette, writable);
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
                on_search=Callback::new(move |_| palette.open.set(true))
                actions=ViewFn::from(move || view! { <palette::actions::ShellActions palette writable /> })
                nav_extra=ViewFn::from(move || {
                    // раздел управления доступом виден только владельцу
                    candidates::list::is_owner(ctx)
                        .then(|| view! { <leptos_router::components::A href="/security">"Безопасность"</leptos_router::components::A> })
                })
            />
            <palette::view::Overlays palette writable />
        </AuthGate>
    }
}

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
