//! Браузерная часть: запросы прямо в BFF по публичному адресу с cookie сессии.
use std::{cell::RefCell, rc::Rc};

use leptos::prelude::*;
use serde_json::Value;

use crate::api::{
    CsrfCache, CsrfPlan, LOGIN_CSRF, LOGIN_PATH, LOGOUT_PATH, Method, Reply, csrf_plan, join_url,
    should_retry_with_fresh_csrf,
};

thread_local! {
    static CSRF: RefCell<CsrfCache> = RefCell::new(CsrfCache::default());
    static ON_UNAUTHORIZED: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Что делать, когда BFF отвечает 401 на запрос данных (сессия истекла): обычно перечитать сессию.
pub fn set_unauthorized_hook(hook: impl Fn() + 'static) {
    ON_UNAUTHORIZED.with(|h| *h.borrow_mut() = Some(Rc::new(hook)));
}

/// Публичный адрес BFF, который сервер положил в страницу.
fn public_url() -> Result<String, ServerFnError> {
    document()
        .query_selector("meta[name=bff-url]")
        .ok()
        .flatten()
        .and_then(|m| m.get_attribute("content"))
        .filter(|u| !u.is_empty())
        .ok_or_else(|| ServerFnError::new("BFF public url is not configured"))
}

fn http_method(method: Method) -> reqwest::Method {
    match method {
        Method::Get => reqwest::Method::GET,
        Method::Post => reqwest::Method::POST,
        Method::Put => reqwest::Method::PUT,
        Method::Patch => reqwest::Method::PATCH,
        Method::Delete => reqwest::Method::DELETE,
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::new()
}

async fn send(
    base: &str,
    method: Method,
    path: &str,
    body: Option<&Value>,
    csrf: Option<&str>,
) -> Result<Reply, ServerFnError> {
    let mut req = http()
        .request(http_method(method), join_url(base, path))
        .fetch_credentials_include();
    if let Some(token) = csrf {
        req = req.header("x-csrf-token", token);
    }
    if let Some(b) = body {
        req = req.json(b);
    }
    let res = req
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("bff unavailable: {e}")))?;
    let status = res.status().as_u16();
    let bytes = res
        .bytes()
        .await
        .map_err(|e| ServerFnError::new(format!("bff read failed: {e}")))?;
    Ok(Reply {
        status,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    })
}

/// CSRF-токен текущей сессии: из кэша либо запросом `/bff/session`.
async fn session_csrf(base: &str, fresh: bool) -> Result<(String, bool), ServerFnError> {
    if !fresh && let Some(token) = CSRF.with(|c| c.borrow().get()) {
        return Ok((token, true));
    }
    let reply = send(base, Method::Get, "/bff/session", None, None).await?;
    let token = reply.body["csrf_token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| ServerFnError::new("not authenticated"))?;
    CSRF.with(|c| c.borrow_mut().set(token.clone()));
    Ok((token, false))
}

pub async fn call(method: Method, path: &str, body: Option<Value>) -> Result<Reply, ServerFnError> {
    let base = public_url()?;
    let plan = csrf_plan(method, path);
    let (token, cached) = match plan {
        CsrfPlan::None => (None, false),
        CsrfPlan::Login => (Some(LOGIN_CSRF.to_string()), false),
        CsrfPlan::Session => {
            let (t, cached) = session_csrf(&base, false).await?;
            (Some(t), cached)
        }
    };
    let mut reply = send(&base, method, path, body.as_ref(), token.as_deref()).await?;
    if should_retry_with_fresh_csrf(reply.status, &plan, cached) {
        let (fresh, _) = session_csrf(&base, true).await?;
        reply = send(&base, method, path, body.as_ref(), Some(&fresh)).await?;
    }
    if path == LOGIN_PATH || path == LOGOUT_PATH {
        CSRF.with(|c| c.borrow_mut().clear());
    }
    if reply.status == 401 && path.starts_with("/api/") {
        ON_UNAUTHORIZED.with(|h| {
            if let Some(hook) = h.borrow().clone() {
                hook();
            }
        });
    }
    Ok(reply)
}
