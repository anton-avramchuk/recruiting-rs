//! CORS для запросов браузера напрямую в BFF: только явно перечисленные источники, с cookie.
use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::Response,
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CorsConfigError {
    #[error("CORS origin {0:?} is not valid: expected scheme://host[:port] without a wildcard")]
    InvalidOrigin(String),
}

/// Разбирает `CORS_ORIGINS`: источники через запятую (`http://localhost:3000, https://crm.example.com`).
/// Подстановка `*`, пустые элементы, пути и схемы кроме http/https отклоняются; хвостовой `/` убирается.
pub fn parse_cors_origins(raw: &str) -> Result<Vec<String>, CorsConfigError> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    raw.split(',').map(|item| normalize(item.trim())).collect()
}

fn normalize(origin: &str) -> Result<String, CorsConfigError> {
    let invalid = || CorsConfigError::InvalidOrigin(origin.to_string());
    let trimmed = origin.trim_end_matches('/');
    let host = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .ok_or_else(invalid)?;
    let plain = !host.is_empty()
        && !host.contains(['*', '/', '?', '#', '@', ' '])
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'));
    if plain {
        Ok(trimmed.to_string())
    } else {
        Err(invalid())
    }
}

const ALLOWED_METHODS: &str = "GET, POST, PUT, PATCH, DELETE";
const ALLOWED_HEADERS: &str = "content-type, x-csrf-token";
/// Сколько браузер может помнить ответ на предзапрос, секунд.
const PREFLIGHT_MAX_AGE: &str = "600";

fn allowed_origin(headers: &HeaderMap, allowed: &[String]) -> Option<HeaderValue> {
    let origin = headers.get(header::ORIGIN)?;
    let text = origin.to_str().ok()?;
    allowed.iter().any(|a| a == text).then(|| origin.clone())
}

fn mark(headers: &mut HeaderMap, origin: HeaderValue) {
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    headers.append(header::VARY, HeaderValue::from_static("Origin"));
}

/// Слой CORS: предзапрос разрешённого источника отвечает сам (до сессии и CSRF),
/// остальным ответам разрешённого источника добавляет заголовки.
pub async fn layer(State(allowed): State<Arc<Vec<String>>>, req: Request, next: Next) -> Response {
    let Some(origin) = allowed_origin(req.headers(), &allowed) else {
        return next.run(req).await;
    };
    let is_preflight = req.method() == Method::OPTIONS
        && req
            .headers()
            .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD);
    if is_preflight {
        let mut res = Response::new(axum::body::Body::empty());
        *res.status_mut() = StatusCode::NO_CONTENT;
        let headers = res.headers_mut();
        mark(headers, origin);
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static(ALLOWED_METHODS),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(ALLOWED_HEADERS),
        );
        headers.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static(PREFLIGHT_MAX_AGE),
        );
        return res;
    }
    let mut res = next.run(req).await;
    mark(res.headers_mut(), origin);
    res
}
