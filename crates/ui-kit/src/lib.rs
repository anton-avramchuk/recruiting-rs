//! Общие Leptos-компоненты и клиент BFF для обоих фронтендов (web-crm, web-admin).
pub mod api;
#[cfg(feature = "hydrate")]
mod client;
mod components;
mod forms;
pub mod grid;
#[cfg(feature = "ssr")]
pub mod server;
mod session;
pub mod theme;

pub use components::{AppShell, BffMeta, NavItem, PageHeader, StatusBadge, UiStyles};
pub use forms::{FormFields, form_fields};
pub use session::{
    AuthGate, LoginOutcome, LoginPage, SessionCtx, SessionInfo, UserInfo, get_session, login,
    logout, provide_session,
};
pub use ui::{ConfirmDialog, Dialog, Drawer, EmptyState, Notice, NoticeKind, Pager};
pub use ui_core::format;
pub use ui_core::{Reply, classify, classify_empty, encode_query_value, typed};

/// Базовый URL BFF для запросов сервера фронтенда (внутренний адрес); кладётся в контекст Leptos.
#[derive(Clone, Debug)]
pub struct BffUrl(pub String);

/// Адрес BFF для браузера; кладётся в контекст Leptos и попадает в страницу мета-тегом.
#[derive(Clone, Debug)]
pub struct BffPublicUrl(pub String);

/// Что делать, когда BFF отвечает 401 на запрос данных из браузера (сессия истекла).
/// На сервере ничего не делает.
pub fn set_unauthorized_hook(hook: impl Fn() + 'static) {
    #[cfg(feature = "hydrate")]
    client::set_unauthorized_hook(hook);
    #[cfg(not(feature = "hydrate"))]
    let _ = hook;
}
