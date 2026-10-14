//! BFF админского слоя: сессии платформенных администраторов, прокси в сервис admin.
use std::{sync::Arc, time::Duration};

use axum::{Json, Router, routing::get};
use bff_core::{AllowRule, Bff, BffConfig, HttpUpstreamAuth, LoginTransform, SessionStore};
use platform_core::error::ApiError;
use serde_json::{Value, json};

pub const COOKIE_NAME: &str = "admin_session";
pub const REDIS_PREFIX: &str = "bff:admin:session:";

#[derive(Debug, Clone)]
pub struct AdminConfig {
    pub admin_url: String,
    pub cookie_secure: bool,
    /// Не больше срока жизни токена администратора: refresh в этом слое не предусмотрен.
    pub session_ttl_secs: u64,
}

/// Маршруты сервиса admin, доступные админскому фронтенду. Internal и пользователи тенантов сюда не входят.
fn allowlist() -> Vec<AllowRule> {
    vec![AllowRule::any("/tenants"), AllowRule::any("/me")]
}

fn login_transform() -> LoginTransform {
    Arc::new(|_headers, body: Value| {
        let field = |name: &str| {
            body[name]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| ApiError::bad_request(format!("{name} is required")))
        };
        Ok(json!({ "email": field("email")?, "password": field("password")? }))
    })
}

pub fn build(cfg: &AdminConfig, store: Arc<dyn SessionStore>) -> Router {
    let bff_config = BffConfig {
        cookie_name: COOKIE_NAME.into(),
        cookie_secure: cfg.cookie_secure,
        session_ttl: Duration::from_secs(cfg.session_ttl_secs),
        upstream_base_url: cfg.admin_url.clone(),
        allow: allowlist(),
        login_transform: Some(login_transform()),
    };
    // у токена администратора нет refresh и отзыва: по истечении нужно войти заново
    let auth = Arc::new(HttpUpstreamAuth::new(
        &cfg.admin_url,
        "/auth/login",
        None,
        None,
    ));
    let doc = bff_core::openapi("Admin BFF");
    bff_core::router(Bff::new(bff_config, store, auth))
        .merge(platform_core::health::router())
        .route("/openapi.json", get(move || async move { Json(doc) }))
}
