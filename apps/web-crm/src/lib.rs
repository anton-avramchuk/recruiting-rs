//! Клиентское приложение рекрутеров (Leptos, SSR + hydrate). Общается только с bff-crm.
use leptos::prelude::*;
use leptos_meta::{MetaTags, Title, provide_meta_context};
use leptos_router::{
    SsrMode,
    components::{ParentRoute, Route, Router, Routes},
    path,
};
use ui_kit::{
    AppShell, AuthGate, EmptyState, LoginPage, NavItem, PageHeader, SessionCtx, UiStyles,
    provide_session,
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
        <Title text="Рекрутинг CRM" />
        <UiStyles />
        <Router>
            <Routes fallback=|| view! { <EmptyState title="Страница не найдена" hint="Проверьте адрес." /> }>
                <Route
                    path=path!("/login")
                    view=|| {
                        view! {
                            <LoginPage
                                with_tenant=true
                                title="Рекрутинг CRM"
                                subtitle="Войдите в рабочее пространство вашей организации."
                            />
                        }
                    }
                    ssr=SsrMode::Async
                />
                <ParentRoute path=path!("") view=Protected ssr=SsrMode::Async>
                    <Route path=path!("/") view=Overview ssr=SsrMode::Async />
                    <Route path=path!("/candidates") view=Candidates ssr=SsrMode::Async />
                    <Route path=path!("/vacancies") view=Vacancies ssr=SsrMode::Async />
                    <Route path=path!("/interviews") view=Interviews ssr=SsrMode::Async />
                </ParentRoute>
            </Routes>
        </Router>
    }
}

fn nav() -> Vec<NavItem> {
    vec![
        NavItem {
            href: "/",
            label: "Обзор",
        },
        NavItem {
            href: "/candidates",
            label: "Кандидаты",
        },
        NavItem {
            href: "/vacancies",
            label: "Вакансии",
        },
        NavItem {
            href: "/interviews",
            label: "Интервью",
        },
    ]
}

/// Защищённая часть: без сессии отправляет на вход, с сессией показывает каркас.
#[component]
fn Protected() -> impl IntoView {
    let ctx = expect_context::<SessionCtx>();
    let user = move || {
        ctx.session
            .get()
            .and_then(Result::ok)
            .and_then(|s| s.user)
            .unwrap_or_default()
    };
    view! {
        <AuthGate>
            <AppShell
                brand="Рекрутинг"
                nav=nav()
                context_label=Signal::derive(move || user().tenant)
                user_label=Signal::derive(move || user().email)
            />
        </AuthGate>
    }
}

#[component]
fn Overview() -> impl IntoView {
    view! {
        <PageHeader title="Обзор">{()}</PageHeader>
        <EmptyState
            title="Здесь появятся задачи на сегодня"
            hint="Раздел заработает после подключения вакансий и кандидатов."
        />
    }
}

#[component]
fn Candidates() -> impl IntoView {
    view! {
        <PageHeader title="Кандидаты">{()}</PageHeader>
        <EmptyState title="Кандидатов пока нет" hint="База кандидатов появится в следующих версиях." />
    }
}

#[component]
fn Vacancies() -> impl IntoView {
    view! {
        <PageHeader title="Вакансии">{()}</PageHeader>
        <EmptyState title="Вакансий пока нет" hint="Создание вакансий появится в следующих версиях." />
    }
}

#[component]
fn Interviews() -> impl IntoView {
    view! {
        <PageHeader title="Интервью">{()}</PageHeader>
        <EmptyState title="Интервью не запланированы" hint="Календарь интервью появится в следующих версиях." />
    }
}

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
