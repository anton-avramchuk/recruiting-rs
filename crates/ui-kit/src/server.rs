//! Серверная часть: вызовы BFF из server functions с пробросом cookie сессии.
use std::sync::OnceLock;

use axum::http::{HeaderMap, HeaderValue, header};
use leptos::prelude::*;
use leptos_axum::ResponseOptions;
use serde_json::Value;

use crate::{
    BffUrl,
    api::{LOGIN_CSRF, LOGIN_PATH, Method},
};

pub use crate::api::BffReply;

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("build http client")
    })
}

fn base_url() -> Result<String, ServerFnError> {
    use_context::<BffUrl>()
        .map(|b| b.0.trim_end_matches('/').to_string())
        .ok_or_else(|| ServerFnError::new("BFF url is not configured"))
}

/// Вызов BFF от имени браузера: cookie сессии берётся из входящего запроса,
/// `Set-Cookie` из ответа уходит обратно в браузер. Для изменяющих запросов
/// CSRF-токен запрашивается у BFF по той же сессии.
pub async fn call(
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<BffReply, ServerFnError> {
    let base = base_url()?;
    let incoming: HeaderMap = leptos_axum::extract().await?;
    let cookie = incoming.get(header::COOKIE).cloned();

    let csrf = if !method.needs_csrf() {
        None
    } else if path == LOGIN_PATH {
        // сессии ещё нет: BFF требует лишь наличие заголовка
        Some(LOGIN_CSRF.to_string())
    } else {
        Some(fetch_csrf(&base, cookie.as_ref()).await?)
    };

    let method = axum::http::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let mut req = http().request(method, crate::api::join_url(&base, path));
    if let Some(c) = &cookie {
        req = req.header(header::COOKIE, c);
    }
    if let Some(token) = csrf {
        req = req.header("x-csrf-token", token);
    }
    if let Some(b) = body {
        req = req.json(&b);
    }
    let res = req
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("bff unavailable: {e}")))?;

    let status = res.status().as_u16();
    if let Some(options) = use_context::<ResponseOptions>() {
        for value in res.headers().get_all(header::SET_COOKIE) {
            options.append_header(header::SET_COOKIE, value.clone());
        }
    }
    let bytes = res
        .bytes()
        .await
        .map_err(|e| ServerFnError::new(format!("bff read failed: {e}")))?;
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Ok(BffReply { status, body })
}

async fn fetch_csrf(base: &str, cookie: Option<&HeaderValue>) -> Result<String, ServerFnError> {
    let mut req = http().get(format!("{base}/bff/session"));
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    let body: Value = req
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("bff unavailable: {e}")))?
        .json()
        .await
        .map_err(|e| ServerFnError::new(format!("bff session unreadable: {e}")))?;
    body["csrf_token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| ServerFnError::new("not authenticated"))
}
