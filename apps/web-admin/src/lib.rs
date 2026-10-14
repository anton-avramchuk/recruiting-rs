//! Приложение платформенных администраторов (Leptos, SSR + hydrate). Общается только с bff-admin.
mod tenants;

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

pub use tenants::{CreateOutcome, StatusOutcome, create_tenant, list_tenants, set_tenant_status};

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
        <Title text="Администрирование платформы" />
        <UiStyles />
        <Router>
            <Routes fallback=|| view! { <EmptyState title="Страница не найдена" hint="Проверьте адрес." /> }>
                <Route
                    path=path!("/login")
                    view=|| {
                        view! {
                            <LoginPage
                                with_tenant=false
                                title="Администрирование платформы"
                                subtitle="Вход для администраторов платформы."
                            />
                        }
                    }
                    ssr=SsrMode::Async
                />
                <ParentRoute path=path!("") view=Protected ssr=SsrMode::Async>
                    <Route path=path!("/") view=tenants::TenantsPage />
                </ParentRoute>
            </Routes>
        </Router>
    }
}

#[component]
fn Protected() -> impl IntoView {
    let ctx = expect_context::<SessionCtx>();
    let email = move || {
        ctx.session
            .get()
            .and_then(Result::ok)
            .and_then(|s| s.user)
            .map(|u| u.email)
            .unwrap_or_default()
    };
    view! {
        <AuthGate>
            <AppShell
                brand="Платформа"
                nav=vec![NavItem { href: "/", label: "Организации" }]
                context_label=Signal::derive(|| "администрирование".to_string())
                user_label=Signal::derive(email)
            />
        </AuthGate>
    }
}

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
