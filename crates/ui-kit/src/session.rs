use leptos::prelude::*;
use leptos_router::{
    components::Redirect,
    hooks::{use_location, use_navigate, use_query_map},
};
use serde::{Deserialize, Serialize};

use crate::components::{Notice, NoticeKind};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UserInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub email: String,
    /// Slug тенанта (только в клиентском слое).
    #[serde(default)]
    pub tenant: String,
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub authenticated: bool,
    pub user: Option<UserInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoginOutcome {
    Success,
    InvalidCredentials,
    /// Организация приостановлена платформой.
    Suspended,
    Unavailable,
}

#[server]
pub async fn get_session() -> Result<SessionInfo, ServerFnError> {
    use axum::http::Method;
    let reply = crate::server::call(Method::GET, "/bff/session", None).await?;
    if reply.body["authenticated"] != true {
        return Ok(SessionInfo::default());
    }
    let user = serde_json::from_value(reply.body["user"].clone()).unwrap_or_default();
    Ok(SessionInfo {
        authenticated: true,
        user: Some(user),
    })
}

#[server]
pub async fn login(
    tenant: String,
    email: String,
    password: String,
) -> Result<LoginOutcome, ServerFnError> {
    use axum::http::Method;
    let mut body = serde_json::json!({ "email": email, "password": password });
    if !tenant.trim().is_empty() {
        body["tenant"] = serde_json::Value::String(tenant.trim().to_string());
    }
    let reply = crate::server::call(Method::POST, "/bff/login", Some(body)).await?;
    Ok(match reply.status {
        200 => LoginOutcome::Success,
        401 | 400 => LoginOutcome::InvalidCredentials,
        403 => LoginOutcome::Suspended,
        _ => LoginOutcome::Unavailable,
    })
}

#[server]
pub async fn logout() -> Result<(), ServerFnError> {
    use axum::http::Method;
    crate::server::call(Method::POST, "/bff/logout", None).await?;
    Ok(())
}

/// Сессия приложения: ресурс, который перезапрашивается после входа/выхода.
#[derive(Clone, Copy)]
pub struct SessionCtx {
    pub session: Resource<Result<SessionInfo, ServerFnError>>,
    version: WriteSignal<u32>,
}

impl SessionCtx {
    /// Перечитать сессию (после входа, выхода или потери авторизации).
    pub fn refresh(&self) {
        self.version.update(|v| *v += 1);
    }
}

pub fn provide_session() -> SessionCtx {
    let (version, set_version) = signal(0u32);
    let session = Resource::new(move || version.get(), |_| get_session());
    let ctx = SessionCtx {
        session,
        version: set_version,
    };
    provide_context(ctx);
    ctx
}

/// Показывает содержимое только аутентифицированным; остальных отправляет на `/login?next=...`.
#[component]
pub fn AuthGate(children: ChildrenFn) -> impl IntoView {
    let ctx = expect_context::<SessionCtx>();
    let location = use_location();
    view! {
        <Suspense fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
            {move || {
                ctx.session
                    .get()
                    .map(|result| match result {
                        Ok(s) if s.authenticated => children().into_any(),
                        _ => {
                            let next = format!(
                                "{}{}",
                                location.pathname.get_untracked(),
                                location.search.get_untracked(),
                            );
                            view! { <Redirect path=login_url(&next) /> }.into_any()
                        }
                    })
            }}
        </Suspense>
    }
}

fn login_url(next: &str) -> String {
    if next.is_empty() || next == "/" {
        "/login".to_string()
    } else {
        format!("/login?next={}", encode_component(next))
    }
}

fn encode_component(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Только внутренние пути: защита от open redirect через `next`.
pub fn safe_next(next: Option<String>) -> String {
    match next {
        Some(n) if n.starts_with('/') && !n.starts_with("//") && !n.contains('\\') => n,
        _ => "/".to_string(),
    }
}

#[component]
pub fn LoginPage(
    /// Показывать поле «Организация» (клиентский слой).
    with_tenant: bool,
    title: &'static str,
    subtitle: &'static str,
) -> impl IntoView {
    let ctx = expect_context::<SessionCtx>();
    let navigate = use_navigate();
    let query = use_query_map();
    let action = ServerAction::<Login>::new();

    Effect::new(move |_| {
        if let Some(Ok(LoginOutcome::Success)) = action.value().get() {
            ctx.refresh();
            navigate(&safe_next(query.get().get("next")), Default::default());
        }
    });

    let error = move || match action.value().get() {
        Some(Ok(LoginOutcome::InvalidCredentials)) => {
            Some("Неверная организация, email или пароль.")
        }
        Some(Ok(LoginOutcome::Suspended)) => {
            Some("Доступ организации приостановлен. Обратитесь к администратору платформы.")
        }
        Some(Ok(LoginOutcome::Unavailable)) | Some(Err(_)) => {
            Some("Сервис временно недоступен. Попробуйте позже.")
        }
        _ => None,
    };

    view! {
        <div class="auth">
            <ActionForm action=action attr:class="card auth__form">
                <h1>{title}</h1>
                <p class="muted">{subtitle}</p>
                {if with_tenant {
                    view! {
                        <label>
                            "Организация"
                            <input name="tenant" autocomplete="organization" required placeholder="acme" />
                        </label>
                    }
                        .into_any()
                } else {
                    view! { <input type="hidden" name="tenant" value="" /> }.into_any()
                }}
                <label>
                    "Email" <input type="email" name="email" autocomplete="username" required />
                </label>
                <label>
                    "Пароль"
                    <input type="password" name="password" autocomplete="current-password" required />
                </label>
                {move || {
                    error()
                        .map(|msg| view! { <Notice kind=NoticeKind::Error>{msg}</Notice> })
                }}
                <button type="submit" class="btn btn--primary" disabled=move || action.pending().get()>
                    {move || if action.pending().get() { "Входим…" } else { "Войти" }}
                </button>
            </ActionForm>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_must_be_a_local_path() {
        assert_eq!(safe_next(Some("/vacancies?x=1".into())), "/vacancies?x=1");
        for bad in [
            "https://evil.com",
            "//evil.com",
            "javascript:alert(1)",
            "\\evil",
            "",
        ] {
            assert_eq!(safe_next(Some(bad.into())), "/", "{bad}");
        }
        assert_eq!(safe_next(None), "/");
    }

    #[test]
    fn login_url_encodes_the_return_path() {
        assert_eq!(login_url("/"), "/login");
        assert_eq!(login_url("/a?b=c d"), "/login?next=/a%3Fb%3Dc%20d");
    }
}
