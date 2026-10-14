use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use platform_core::error::ApiError;
use serde_json::{Value, json};

use crate::{
    config::BffConfig,
    session::{Session, SessionStore, random_token},
    upstream::{Grant, UpstreamAuth, UpstreamError},
};

/// За сколько секунд до истечения access-токен обновляется заранее.
const REFRESH_SKEW_SECS: i64 = 30;
pub const CSRF_HEADER: &str = "x-csrf-token";

pub(crate) struct Inner {
    pub config: BffConfig,
    pub store: Arc<dyn SessionStore>,
    pub auth: Arc<dyn UpstreamAuth>,
    pub http: reqwest::Client,
    /// Обновление токенов сериализуется по сессии: параллельные запросы не должны
    /// предъявлять один refresh-токен дважды (это выглядит как кража и отзывает цепочку).
    refresh_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

#[derive(Clone)]
pub struct Bff {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs() as i64
}

impl Bff {
    pub fn new(
        config: BffConfig,
        store: Arc<dyn SessionStore>,
        auth: Arc<dyn UpstreamAuth>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                config,
                store,
                auth,
                http: reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .expect("build http client"),
                refresh_locks: Mutex::default(),
            }),
        }
    }

    // ---- cookie ----

    pub(crate) fn session_id(&self, headers: &HeaderMap) -> Option<String> {
        let name = &self.inner.config.cookie_name;
        headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(';'))
            .filter_map(|pair| pair.trim().split_once('='))
            .find(|(n, v)| n == name && !v.is_empty())
            .map(|(_, v)| v.to_string())
    }

    fn cookie(&self, value: &str, max_age_secs: u64) -> HeaderValue {
        let secure = if self.inner.config.cookie_secure {
            " Secure;"
        } else {
            ""
        };
        HeaderValue::from_str(&format!(
            "{}={value}; HttpOnly;{secure} SameSite=Lax; Path=/; Max-Age={max_age_secs}",
            self.inner.config.cookie_name
        ))
        .expect("valid cookie header")
    }

    pub(crate) fn clear_cookie(&self) -> HeaderValue {
        self.cookie("", 0)
    }

    // ---- сессия ----

    /// Текущая сессия запроса (без обновления токенов).
    pub(crate) async fn load(
        &self,
        headers: &HeaderMap,
    ) -> Result<Option<(String, Session)>, ApiError> {
        let Some(id) = self.session_id(headers) else {
            return Ok(None);
        };
        let session = self
            .inner
            .store
            .get(&id)
            .await
            .map_err(|e| ApiError::internal(&e))?;
        Ok(session.map(|s| (id, s)))
    }

    /// Возвращает сессию со свежим access-токеном; при невозможности обновить удаляет сессию.
    pub(crate) async fn ensure_fresh(
        &self,
        id: &str,
        session: Session,
    ) -> Result<Session, Freshness> {
        if now_secs() + REFRESH_SKEW_SECS < session.access_expires_at {
            return Ok(session);
        }
        self.refresh(id, Some(&session.access_token)).await
    }

    /// Принудительное обновление. `stale_access` - токен, который не сработал: если сессия
    /// уже обновлена другим запросом, повторного обращения к бэкенду не будет.
    pub(crate) async fn refresh(
        &self,
        id: &str,
        stale_access: Option<&str>,
    ) -> Result<Session, Freshness> {
        let lock = {
            let mut locks = self.inner.refresh_locks.lock().expect("locks");
            locks.entry(id.to_string()).or_default().clone()
        };
        let _guard = lock.lock().await;
        let result = self.refresh_locked(id, stale_access).await;
        drop(_guard);
        // чистим карту, если никто больше не ждёт этот замок
        let mut locks = self.inner.refresh_locks.lock().expect("locks");
        if locks.get(id).is_some_and(|l| Arc::strong_count(l) <= 2) {
            locks.remove(id);
        }
        result
    }

    async fn refresh_locked(
        &self,
        id: &str,
        stale_access: Option<&str>,
    ) -> Result<Session, Freshness> {
        let current = self
            .inner
            .store
            .get(id)
            .await
            .map_err(|e| Freshness::Failed(ApiError::internal(&e)))?
            .ok_or(Freshness::Expired)?;
        // кто-то уже обновил сессию, пока мы ждали замок
        let fresh_enough = now_secs() + REFRESH_SKEW_SECS < current.access_expires_at;
        if fresh_enough && stale_access.is_none_or(|stale| stale != current.access_token) {
            return Ok(current);
        }
        let Some(refresh_token) = current.refresh_token.clone() else {
            self.destroy(id).await;
            return Err(Freshness::Expired);
        };
        match self.inner.auth.refresh(&refresh_token).await {
            Ok(grant) => {
                let updated = apply_grant(&current, grant);
                self.inner
                    .store
                    .save(id, &updated, self.inner.config.session_ttl)
                    .await
                    .map_err(|e| Freshness::Failed(ApiError::internal(&e)))?;
                Ok(updated)
            }
            Err(UpstreamError::Rejected { .. }) => {
                self.destroy(id).await;
                Err(Freshness::Expired)
            }
            Err(UpstreamError::Unavailable(m)) => {
                tracing::error!(error = %m, "token refresh failed: upstream unavailable");
                Err(Freshness::Failed(ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    "Bad Gateway",
                )))
            }
        }
    }

    pub(crate) async fn destroy(&self, id: &str) {
        if let Err(e) = self.inner.store.delete(id).await {
            tracing::error!(error = %e, "failed to delete session");
        }
    }

    // ---- CSRF ----

    /// Изменяющие запросы обязаны нести заголовок; при наличии сессии он должен совпадать с её токеном.
    /// Браузер не может добавить произвольный заголовок к кросс-доменному запросу без CORS.
    pub(crate) fn check_csrf(
        headers: &HeaderMap,
        session: Option<&Session>,
    ) -> Result<(), ApiError> {
        let given = headers
            .get(CSRF_HEADER)
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.is_empty())
            .ok_or_else(ApiError::forbidden)?;
        match session {
            Some(s) if !constant_time_eq(given, &s.csrf_token) => Err(ApiError::forbidden()),
            _ => Ok(()),
        }
    }
}

pub(crate) enum Freshness {
    /// Сессии больше нет (истекла, отозвана, тенант приостановлен).
    Expired,
    Failed(ApiError),
}

impl Freshness {
    pub(crate) fn into_response(self, bff: &Bff) -> Response {
        match self {
            Freshness::Expired => {
                let mut res = ApiError::unauthorized().into_response();
                res.headers_mut()
                    .append(header::SET_COOKIE, bff.clear_cookie());
                res
            }
            Freshness::Failed(e) => e.into_response(),
        }
    }
}

fn apply_grant(previous: &Session, grant: Grant) -> Session {
    Session {
        access_token: grant.access_token,
        // если бэкенд не выдал новый refresh-токен, остаётся прежний
        refresh_token: grant
            .refresh_token
            .or_else(|| previous.refresh_token.clone()),
        access_expires_at: now_secs() + grant.expires_in,
        csrf_token: previous.csrf_token.clone(),
    }
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// Безопасные поля из payload JWT для интерфейса (подпись не проверяется: токен получен от бэкенда).
pub(crate) fn user_preview(access_token: &str) -> Value {
    let payload = access_token
        .split('.')
        .nth(1)
        .and_then(|p| URL_SAFE_NO_PAD.decode(p).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .unwrap_or(Value::Null);
    json!({
        "id": payload["sub"],
        "tenant_id": payload["tid"],
        "roles": payload["roles"],
    })
}

// ---- обработчики /bff/* ----

pub fn router(bff: Bff) -> Router {
    Router::new()
        .route("/bff/login", post(login))
        .route("/bff/logout", post(logout))
        .route("/bff/session", get(session))
        .merge(crate::proxy::routes())
        .with_state(bff)
}

async fn login(State(bff): State<Bff>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    if let Err(e) = Bff::check_csrf(&headers, None) {
        return e.into_response();
    }
    let credentials = match &bff.inner.config.login_transform {
        Some(transform) => match transform(&headers, body) {
            Ok(v) => v,
            Err(e) => return e.into_response(),
        },
        None => body,
    };
    let grant = match bff.inner.auth.login(credentials).await {
        Ok(g) => g,
        Err(UpstreamError::Rejected { status }) => {
            let code = if status == 403 {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::UNAUTHORIZED
            };
            return ApiError::new(
                code,
                if code == StatusCode::FORBIDDEN {
                    "Forbidden"
                } else {
                    "Unauthorized"
                },
            )
            .into_response();
        }
        Err(UpstreamError::Unavailable(m)) => {
            tracing::error!(error = %m, "login failed: upstream unavailable");
            return ApiError::new(StatusCode::BAD_GATEWAY, "Bad Gateway").into_response();
        }
    };

    let session = Session {
        access_token: grant.access_token,
        refresh_token: grant.refresh_token,
        access_expires_at: now_secs() + grant.expires_in,
        csrf_token: random_token(),
    };
    let id = match bff
        .inner
        .store
        .create(&session, bff.inner.config.session_ttl)
        .await
    {
        Ok(id) => id,
        Err(e) => return ApiError::internal(&e).into_response(),
    };
    let mut res = Json(json!({
        "authenticated": true,
        "csrf_token": session.csrf_token,
        "user": user_preview(&session.access_token),
    }))
    .into_response();
    res.headers_mut().append(
        header::SET_COOKIE,
        bff.cookie(&id, bff.inner.config.session_ttl.as_secs()),
    );
    res
}

async fn logout(State(bff): State<Bff>, headers: HeaderMap) -> Response {
    let (id, session) = match bff.load(&headers).await {
        Ok(Some(found)) => found,
        Ok(None) => return no_content_clearing(&bff),
        Err(e) => return e.into_response(),
    };
    if let Err(e) = Bff::check_csrf(&headers, Some(&session)) {
        return e.into_response();
    }
    if let Some(refresh) = &session.refresh_token
        && let Err(e) = bff.inner.auth.logout(refresh).await
    {
        tracing::warn!(error = %e, "upstream logout failed");
    }
    bff.destroy(&id).await;
    no_content_clearing(&bff)
}

fn no_content_clearing(bff: &Bff) -> Response {
    let mut res = StatusCode::NO_CONTENT.into_response();
    res.headers_mut()
        .append(header::SET_COOKIE, bff.clear_cookie());
    res
}

async fn session(State(bff): State<Bff>, headers: HeaderMap) -> Response {
    let (id, session) = match bff.load(&headers).await {
        Ok(Some(found)) => found,
        Ok(None) => return Json(json!({ "authenticated": false })).into_response(),
        Err(e) => return e.into_response(),
    };
    match bff.ensure_fresh(&id, session).await {
        Ok(s) => Json(json!({
            "authenticated": true,
            "csrf_token": s.csrf_token,
            "user": user_preview(&s.access_token),
        }))
        .into_response(),
        Err(Freshness::Expired) => {
            let mut res = Json(json!({ "authenticated": false })).into_response();
            res.headers_mut()
                .append(header::SET_COOKIE, bff.clear_cookie());
            res
        }
        Err(Freshness::Failed(e)) => e.into_response(),
    }
}
