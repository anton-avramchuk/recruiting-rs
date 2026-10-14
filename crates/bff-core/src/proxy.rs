//! Проксирование `/api/*` в бэкенд с подстановкой токена сессии.
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Request, State},
    http::{HeaderMap, HeaderName, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::any,
};
use platform_core::error::ApiError;

use crate::bff::{Bff, Freshness};

const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const PREFIX: &str = "/api";

pub(crate) fn routes() -> Router<Bff> {
    Router::new().route("/api/{*rest}", any(forward))
}

/// Заголовки запроса, которые передаются в бэкенд. Authorization клиента намеренно не входит.
const REQUEST_HEADERS: &[HeaderName] = &[
    header::CONTENT_TYPE,
    header::ACCEPT,
    header::ACCEPT_LANGUAGE,
    header::IF_NONE_MATCH,
];

fn relay_response_header(name: &HeaderName) -> bool {
    matches!(
        *name,
        header::CONTENT_TYPE
            | header::CACHE_CONTROL
            | header::ETAG
            | header::LAST_MODIFIED
            | header::CONTENT_DISPOSITION
    ) || name.as_str().starts_with("x-")
}

fn is_safe_method(m: &Method) -> bool {
    matches!(*m, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// Путь для бэкенда или `None`, если запрос подозрителен (обход каталогов и кодированные разделители).
fn upstream_path(uri: &Uri) -> Option<String> {
    let raw = uri.path();
    let lower = raw.to_ascii_lowercase();
    if lower.contains("%2e") || lower.contains("%2f") || lower.contains("%5c") || raw.contains('\\')
    {
        return None;
    }
    let rest = raw.strip_prefix(PREFIX)?;
    if rest.is_empty() || !rest.starts_with('/') {
        return None;
    }
    if rest.split('/').any(|seg| seg == ".." || seg == ".") {
        return None;
    }
    Some(rest.to_string())
}

async fn forward(State(bff): State<Bff>, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let (method, uri, headers) = (parts.method, parts.uri, parts.headers);

    let Some(path) = upstream_path(&uri) else {
        return ApiError::not_found().into_response();
    };
    let config = &bff.inner.config;
    let Some(rule) = config.allow.iter().find(|r| r.allows(&method, &path)) else {
        return ApiError::not_found().into_response();
    };
    let base_url = match rule.upstream() {
        None => config.upstream_base_url.clone(),
        Some(name) => match config.upstreams.get(name) {
            Some(url) => url.clone(),
            None => {
                tracing::error!(upstream = name, "allowlist names an unknown upstream");
                return ApiError::new(StatusCode::BAD_GATEWAY, "Bad Gateway").into_response();
            }
        },
    };

    let (id, session) = match bff.load(&headers).await {
        Ok(Some(found)) => found,
        Ok(None) => return ApiError::unauthorized().into_response(),
        Err(e) => return e.into_response(),
    };
    if !is_safe_method(&method)
        && let Err(e) = Bff::check_csrf(&headers, Some(&session))
    {
        return e.into_response();
    }
    let body = match axum::body::to_bytes(body, MAX_BODY_BYTES).await {
        Ok(b) => b,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };

    let session = match bff.ensure_fresh(&id, session).await {
        Ok(s) => s,
        Err(f) => return f.into_response(&bff),
    };

    let url = match uri.query() {
        Some(q) => format!("{base_url}{path}?{q}"),
        None => format!("{base_url}{path}"),
    };

    let mut reply = match send(&bff, &method, &url, &headers, &body, &session.access_token).await {
        Ok(r) => r,
        Err(e) => return e.into_response(),
    };

    // токен мог быть отозван на стороне бэкенда: одно обновление и повтор
    if reply.status() == reqwest::StatusCode::UNAUTHORIZED {
        match bff.refresh(&id, Some(&session.access_token)).await {
            Ok(fresh) => {
                reply = match send(&bff, &method, &url, &headers, &body, &fresh.access_token).await
                {
                    Ok(r) => r,
                    Err(e) => return e.into_response(),
                };
            }
            Err(Freshness::Expired) => return Freshness::Expired.into_response(&bff),
            Err(f) => return f.into_response(&bff),
        }
    }

    relay(reply).await
}

async fn send(
    bff: &Bff,
    method: &Method,
    url: &str,
    headers: &HeaderMap,
    body: &Bytes,
    access_token: &str,
) -> Result<reqwest::Response, ApiError> {
    let mut rb = bff
        .inner
        .http
        .request(method.clone(), url)
        .bearer_auth(access_token);
    for name in REQUEST_HEADERS {
        if let Some(v) = headers.get(name) {
            rb = rb.header(name, v);
        }
    }
    if !body.is_empty() {
        rb = rb.body(body.clone());
    }
    rb.send().await.map_err(|e| {
        tracing::error!(error = %e, "upstream request failed");
        ApiError::new(StatusCode::BAD_GATEWAY, "Bad Gateway")
    })
}

async fn relay(reply: reqwest::Response) -> Response {
    let status = StatusCode::from_u16(reply.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut out_headers = HeaderMap::new();
    for (name, value) in reply.headers() {
        if relay_response_header(name) {
            out_headers.append(name.clone(), value.clone());
        }
    }
    match reply.bytes().await {
        Ok(bytes) => {
            let mut res = Response::new(Body::from(bytes));
            *res.status_mut() = status;
            *res.headers_mut() = out_headers;
            res
        }
        Err(e) => {
            tracing::error!(error = %e, "reading upstream body failed");
            ApiError::new(StatusCode::BAD_GATEWAY, "Bad Gateway").into_response()
        }
    }
}
