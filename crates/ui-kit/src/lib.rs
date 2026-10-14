//! Общие Leptos-компоненты и клиент BFF для обоих фронтендов (web-crm, web-admin).
mod components;
mod encode;
#[cfg(feature = "ssr")]
pub mod server;
mod session;

pub use components::{
    AppShell, EmptyState, NavItem, Notice, NoticeKind, PageHeader, StatusBadge, UiStyles,
};
pub use encode::encode_query_value;
pub use session::{
    AuthGate, LoginOutcome, LoginPage, SessionCtx, SessionInfo, UserInfo, get_session, login,
    logout, provide_session,
};

/// Базовый URL BFF; сервер фронтенда кладёт его в контекст Leptos.
#[derive(Clone, Debug)]
pub struct BffUrl(pub String);
