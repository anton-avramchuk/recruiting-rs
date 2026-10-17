//! Помощники тестов BFF (feature `test-support`): мок бэкенда и клиент с cookie.
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    routing::{get, post},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

/// Мок identity: выдаёт токены acc-N/ref-N, помнит действительные и считает вызовы.
#[derive(Clone, Default)]
pub struct Upstream {
    pub issued: Arc<AtomicUsize>,
    pub valid_access: Arc<Mutex<HashSet<String>>>,
    pub valid_refresh: Arc<Mutex<HashSet<String>>>,
    pub refresh_calls: Arc<AtomicUsize>,
    pub logout_calls: Arc<Mutex<Vec<String>>>,
    pub api_calls: Arc<AtomicUsize>,
    pub last_auth_header: Arc<Mutex<Option<String>>>,
    pub last_login_body: Arc<Mutex<Option<Value>>>,
    /// Строка запроса последнего вызова API (после `?`).
    pub last_query: Arc<Mutex<Option<String>>>,
    pub access_ttl_secs: Arc<Mutex<i64>>,
    /// Если задан - refresh отвечает этим статусом.
    pub refresh_status: Arc<Mutex<Option<u16>>>,
}

impl Upstream {
    pub fn new() -> Self {
        let u = Self::default();
        *u.access_ttl_secs.lock().unwrap() = 600;
        u
    }

    fn issue(&self) -> Value {
        let n = self.issued.fetch_add(1, Ordering::SeqCst) + 1;
        let (a, r) = (format!("acc-{n}"), format!("ref-{n}"));
        self.valid_access.lock().unwrap().insert(a.clone());
        self.valid_refresh.lock().unwrap().insert(r.clone());
        json!({"access_token": a, "refresh_token": r, "expires_in": *self.access_ttl_secs.lock().unwrap()})
    }

    pub fn revoke_access(&self, token: &str) {
        self.valid_access.lock().unwrap().remove(token);
    }

    pub async fn serve(&self) -> String {
        async fn login(
            State(u): State<Upstream>,
            Json(body): Json<Value>,
        ) -> (StatusCode, Json<Value>) {
            *u.last_login_body.lock().unwrap() = Some(body.clone());
            if body["password"] == "good-password" {
                (StatusCode::OK, Json(u.issue()))
            } else {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"title": "Unauthorized", "status": 401})),
                )
            }
        }
        async fn refresh(
            State(u): State<Upstream>,
            Json(body): Json<Value>,
        ) -> (StatusCode, Json<Value>) {
            u.refresh_calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(50)).await; // даём шанс гонке
            if let Some(code) = *u.refresh_status.lock().unwrap() {
                return (
                    StatusCode::from_u16(code).unwrap(),
                    Json(json!({"title": "no"})),
                );
            }
            let token = body["refresh_token"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            if u.valid_refresh.lock().unwrap().remove(&token) {
                (StatusCode::OK, Json(u.issue()))
            } else {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"title": "Unauthorized"})),
                )
            }
        }
        async fn logout(State(u): State<Upstream>, Json(body): Json<Value>) -> StatusCode {
            u.logout_calls.lock().unwrap().push(
                body["refresh_token"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            );
            StatusCode::NO_CONTENT
        }
        async fn users(
            State(u): State<Upstream>,
            uri: axum::http::Uri,
            headers: HeaderMap,
        ) -> (StatusCode, HeaderMap, Json<Value>) {
            u.api_calls.fetch_add(1, Ordering::SeqCst);
            *u.last_query.lock().unwrap() = uri.query().map(str::to_string);
            let auth = headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            *u.last_auth_header.lock().unwrap() = auth.clone();
            let ok = auth
                .as_deref()
                .and_then(|a| a.strip_prefix("Bearer "))
                .is_some_and(|t| u.valid_access.lock().unwrap().contains(t));
            let mut out = HeaderMap::new();
            out.insert("set-cookie", "upstream=leak".parse().unwrap());
            out.insert("x-safe", "yes".parse().unwrap());
            if ok {
                (StatusCode::OK, out, Json(json!({"users": ["anna"]})))
            } else {
                (
                    StatusCode::UNAUTHORIZED,
                    out,
                    Json(json!({"title": "Unauthorized"})),
                )
            }
        }
        async fn secret(State(u): State<Upstream>) -> Json<Value> {
            u.api_calls.fetch_add(1, Ordering::SeqCst);
            Json(json!({"secret": true}))
        }
        let app = Router::new()
            .route("/auth/login", post(login))
            .route("/auth/refresh", post(refresh))
            .route("/auth/logout", post(logout))
            .route("/users", get(users).post(users))
            .route("/users/1", get(users).patch(users))
            .route("/roles", get(users).post(users))
            .route("/roles/1", get(users).patch(users).delete(users))
            .route("/tenants", get(users).post(users))
            .route("/me", get(users))
            .route("/persons", get(users).post(users))
            .route("/vacancies", get(users).post(users))
            .route("/vacancies/1", get(users).post(users))
            .route("/applications", get(users).post(users))
            .route("/applications/grid", get(users))
            .route("/applications/people", get(users))
            .route("/applications/1/stage", get(users).post(users))
            .route("/overview", get(users))
            .route("/interviews", get(users))
            .route("/analytics", get(users))
            .route("/comments", get(users).post(users))
            .route("/comments/1", get(users).patch(users).delete(users))
            .route("/clients", get(users).post(users))
            .route("/clients/grid", get(users))
            .route("/clients/1", get(users).patch(users))
            .route("/clients/1/contacts", post(users))
            .route("/contacts/1", get(users).patch(users).delete(users))
            .route("/positions", get(users).post(users))
            .route("/positions/1", get(users).patch(users).delete(users))
            .route("/admin/secret", get(secret))
            .with_state(self.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }
}

/// Клиент с банкой cookie; возвращает статус, заголовки и JSON-тело.
#[derive(Clone)]
pub struct Client {
    router: Router,
    cookie_name: String,
    pub cookie: Arc<Mutex<Option<String>>>,
}

pub struct Reply {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Value,
    pub raw: String,
}

impl Client {
    pub fn new(router: Router, cookie_name: &str) -> Self {
        Self {
            router,
            cookie_name: cookie_name.into(),
            cookie: Arc::default(),
        }
    }

    pub async fn send(
        &self,
        method: &str,
        uri: &str,
        extra: &[(&str, &str)],
        body: Option<Value>,
    ) -> Reply {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(c) = self.cookie.lock().unwrap().clone() {
            req = req.header("cookie", c);
        }
        for (k, v) in extra {
            req = req.header(*k, *v);
        }
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let res = self
            .router
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = res.status().as_u16();
        let headers = res.headers().clone();
        for sc in headers.get_all("set-cookie") {
            let sc = sc.to_str().unwrap();
            let pair = sc.split(';').next().unwrap();
            let (name, value) = pair.split_once('=').unwrap();
            if name == self.cookie_name {
                *self.cookie.lock().unwrap() = if value.is_empty() {
                    None
                } else {
                    Some(pair.to_string())
                };
            }
        }
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let raw = String::from_utf8_lossy(&bytes).to_string();
        Reply {
            status,
            headers,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            raw,
        }
    }

    pub async fn login(&self) -> Reply {
        self.send(
            "POST",
            "/bff/login",
            &[("x-csrf-token", "login")],
            Some(json!({"email": "a@b.io", "password": "good-password"})),
        )
        .await
    }

    pub fn session_cookie_value(&self) -> Option<String> {
        self.cookie
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.split_once('=').map(|(_, v)| v.to_string()))
    }
}
