use leptos::prelude::*;
use leptos_router::{
    components::Redirect,
    hooks::{use_location, use_navigate, use_query_map},
};
use serde::{Deserialize, Serialize};

use crate::{
    api::{self, Method},
    components::{Notice, NoticeKind},
    forms::form_fields,
};

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
    /// Права собственной роли организации (`read_data`, `write_data`, `delete_data`); у встроенных ролей нет.
    #[serde(default)]
    pub perms: Option<Vec<String>>,
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

pub async fn get_session() -> Result<SessionInfo, ServerFnError> {
    let reply = api::call(Method::Get, "/bff/session", None).await?;
    if reply.body["authenticated"] != true {
        return Ok(SessionInfo::default());
    }
    let user = serde_json::from_value(reply.body["user"].clone()).unwrap_or_default();
    Ok(SessionInfo {
        authenticated: true,
        user: Some(user),
    })
}

pub async fn login(
    tenant: String,
    email: String,
    password: String,
) -> Result<LoginOutcome, ServerFnError> {
    let mut body = serde_json::json!({ "email": email, "password": password });
    if !tenant.trim().is_empty() {
        body["tenant"] = serde_json::Value::String(tenant.trim().to_string());
    }
    let reply = api::call(Method::Post, api::LOGIN_PATH, Some(body)).await?;
    Ok(match reply.status {
        200 => LoginOutcome::Success,
        401 | 400 => LoginOutcome::InvalidCredentials,
        403 => LoginOutcome::Suspended,
        _ => LoginOutcome::Unavailable,
    })
}

pub async fn logout() -> Result<(), ServerFnError> {
    api::call(Method::Post, api::LOGOUT_PATH, None).await?;
    Ok(())
}

/// Сессия приложения: ресурс, который перезапрашивается после входа/выхода.
#[derive(Clone, Copy)]
pub struct SessionCtx {
    /// Результат вместе с версией, для которой он получен.
    session: Resource<(u32, Result<SessionInfo, ServerFnError>)>,
    version: RwSignal<u32>,
}

impl SessionCtx {
    /// Перечитать сессию (после входа, выхода или потери авторизации).
    pub fn refresh(&self) {
        self.version.update(|v| *v += 1);
    }

    /// Актуальная сессия или `None`, пока идёт загрузка. Устаревший ответ (до входа/выхода)
    /// не возвращается: иначе сразу после входа защищённый маршрут успел бы отправить обратно на вход.
    pub fn current(&self) -> Option<Result<SessionInfo, ServerFnError>> {
        let (version, result) = self.session.get()?;
        (version == self.version.get()).then_some(result)
    }

    /// Загружен ли актуальный ответ (для `Suspense`-подобных условий).
    pub fn resource(&self) -> Resource<(u32, Result<SessionInfo, ServerFnError>)> {
        self.session
    }
}

pub fn provide_session() -> SessionCtx {
    let version = RwSignal::new(0u32);
    let session = Resource::new(
        move || version.get(),
        |v| async move { (v, get_session().await) },
    );
    let ctx = SessionCtx { session, version };
    provide_context(ctx);
    // сессия истекла посреди работы: перечитываем её, и AuthGate отправит на вход
    crate::set_unauthorized_hook(move || ctx.refresh());
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
                // пока ответ устарел, не решаем ничего: Suspense держит fallback
                let _ = ctx.resource().get();
                ctx.current()
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
    let action = Action::new(|(tenant, email, password): &(String, String, String)| {
        let (tenant, email, password) = (tenant.clone(), email.clone(), password.clone());
        async move { login(tenant, email, password).await }
    });
    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let fields = form_fields(&ev);
        let field = |name: &str| fields.get(name).cloned().unwrap_or_default();
        action.dispatch((field("tenant"), field("email"), field("password")));
    };

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
            // method="post" и return false: до гидратации пароль не должен уйти в адресную строку
            <form class="card auth__form" method="post" on:submit=on_submit onsubmit="return false">
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
            </form>
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
