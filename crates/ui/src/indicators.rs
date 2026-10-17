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
