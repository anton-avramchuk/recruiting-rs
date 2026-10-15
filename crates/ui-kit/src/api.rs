//! Доступ к BFF одной функцией для сервера (SSR) и браузера.
//!
//! На сервере запрос уходит по внутреннему адресу с cookie входящего запроса; в браузере —
//! напрямую в BFF по публичному адресу с `credentials: include` и CSRF-токеном.
use leptos::prelude::*;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }

    /// Изменяющим запросам нужен CSRF-токен.
    pub fn needs_csrf(self) -> bool {
        self != Method::Get
    }
}

/// Ответ BFF: код и тело (JSON либо `null`).
pub struct Reply {
    pub status: u16,
    pub body: Value,
}

/// Совместимое имя ответа для кода, написанного до появления `api`.
pub type BffReply = Reply;

/// Склеивает базовый адрес и путь запроса: ровно один `/` между ними.
pub fn join_url(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// Вход выполняется до появления сессии: BFF требует лишь наличие заголовка.
pub const LOGIN_PATH: &str = "/bff/login";
pub const LOGOUT_PATH: &str = "/bff/logout";
pub const LOGIN_CSRF: &str = "login";

/// Какой токен отправлять с запросом.
#[derive(Debug, PartialEq, Eq)]
pub enum CsrfPlan {
    /// Безопасный метод: заголовок не нужен.
    None,
    /// Вход: фиксированное значение.
    Login,
    /// Токен сессии (из кэша либо запросом `/bff/session`).
    Session,
}

pub fn csrf_plan(method: Method, path: &str) -> CsrfPlan {
    if !method.needs_csrf() {
        CsrfPlan::None
    } else if path == LOGIN_PATH {
        CsrfPlan::Login
    } else {
        CsrfPlan::Session
    }
}

/// Нужно ли один раз повторить запрос со свежим токеном: BFF ответил 403, а токен был из кэша
/// (мог устареть после входа в другой вкладке или перезапуска сессии).
pub fn should_retry_with_fresh_csrf(status: u16, plan: &CsrfPlan, token_was_cached: bool) -> bool {
    status == 403 && *plan == CsrfPlan::Session && token_was_cached
}

/// Кэш CSRF-токена на время жизни страницы.
#[derive(Debug, Default)]
pub struct CsrfCache {
    token: Option<String>,
}

impl CsrfCache {
    pub fn get(&self) -> Option<String> {
        self.token.clone()
    }

    pub fn set(&mut self, token: String) {
        self.token = Some(token);
    }

    /// После входа и выхода токен другой.
    pub fn clear(&mut self) {
        self.token = None;
    }
}

/// Обращение к BFF. `path` — путь BFF (`/bff/session`, `/api/vacancies?...`).
pub async fn call(method: Method, path: &str, body: Option<Value>) -> Result<Reply, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::server::call(method, path, body).await
    }
    #[cfg(all(not(feature = "ssr"), feature = "hydrate"))]
    {
        send_wrapper::SendWrapper::new(crate::client::call(method, path, body)).await
    }
    #[cfg(all(not(feature = "ssr"), not(feature = "hydrate")))]
    {
        let _ = (method, path, body);
        Err(ServerFnError::new(
            "no transport: build with ssr or hydrate",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_joined_with_a_single_slash() {
        assert_eq!(join_url("http://h:1", "/api/x"), "http://h:1/api/x");
        assert_eq!(join_url("http://h:1/", "/api/x"), "http://h:1/api/x");
        assert_eq!(join_url("http://h:1/", "api/x?y=1"), "http://h:1/api/x?y=1");
    }

    #[test]
    fn only_mutations_need_csrf_and_login_uses_a_fixed_value() {
        assert_eq!(csrf_plan(Method::Get, "/api/vacancies"), CsrfPlan::None);
        assert_eq!(csrf_plan(Method::Post, "/api/vacancies"), CsrfPlan::Session);
        assert_eq!(
            csrf_plan(Method::Patch, "/api/persons/1"),
            CsrfPlan::Session
        );
        assert_eq!(
            csrf_plan(Method::Delete, "/api/persons/1"),
            CsrfPlan::Session
        );
        assert_eq!(csrf_plan(Method::Post, "/bff/login"), CsrfPlan::Login);
        assert_eq!(csrf_plan(Method::Post, "/bff/logout"), CsrfPlan::Session);
    }

    #[test]
    fn a_stale_cached_token_is_retried_once() {
        assert!(should_retry_with_fresh_csrf(403, &CsrfPlan::Session, true));
        assert!(!should_retry_with_fresh_csrf(
            403,
            &CsrfPlan::Session,
            false
        ));
        assert!(!should_retry_with_fresh_csrf(403, &CsrfPlan::Login, true));
        assert!(!should_retry_with_fresh_csrf(200, &CsrfPlan::Session, true));
        assert!(!should_retry_with_fresh_csrf(401, &CsrfPlan::Session, true));
    }

    #[test]
    fn the_token_cache_is_cleared_explicitly() {
        let mut cache = CsrfCache::default();
        assert_eq!(cache.get(), None);
        cache.set("abc".into());
        assert_eq!(cache.get().as_deref(), Some("abc"));
        cache.clear();
        assert_eq!(cache.get(), None);
    }

    #[test]
    fn methods_know_their_names() {
        assert_eq!(Method::Patch.as_str(), "PATCH");
        assert!(!Method::Get.needs_csrf());
        assert!(Method::Delete.needs_csrf());
    }
}
