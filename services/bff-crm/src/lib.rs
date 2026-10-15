//! BFF клиентского слоя: сессии пользователей тенантов, прокси в identity.
use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::{Json, Router, http::HeaderMap, routing::get};
use bff_core::{AllowRule, Bff, BffConfig, HttpUpstreamAuth, LoginTransform, SessionStore};
use platform_core::error::ApiError;
use serde_json::{Value, json};

pub const COOKIE_NAME: &str = "crm_session";
pub const REDIS_PREFIX: &str = "bff:crm:session:";

#[derive(Debug, Clone)]
pub struct CrmConfig {
    pub identity_url: String,
    pub candidate_url: String,
    pub recruitment_url: String,
    /// Источники браузера для запросов напрямую в BFF (пусто - CORS выключен).
    pub cors_origins: Vec<String>,
    pub cookie_secure: bool,
    /// Базовый домен приложения: `acme.<base_domain>` определяет тенанта `acme`.
    pub base_domain: Option<String>,
    pub session_ttl_secs: u64,
}

/// Маршруты бэкенда, доступные клиентскому фронтенду. Admin, internal и auth сюда не входят.
fn allowlist() -> Vec<AllowRule> {
    vec![
        AllowRule::any("/users"),
        AllowRule::any("/persons").to("candidate"),
        AllowRule::any("/vacancies").to("recruitment"),
        AllowRule::any("/applications").to("recruitment"),
        AllowRule::any("/overview").to("recruitment"),
        AllowRule::any("/interviews").to("recruitment"),
    ]
}

/// `acme.crm.example.com` при базе `crm.example.com` -> `acme`.
pub fn tenant_from_host<'a>(host: &'a str, base_domain: &str) -> Option<&'a str> {
    let host = host.split(':').next().unwrap_or(host).trim_end_matches('.');
    let sub = host.strip_suffix(base_domain)?.strip_suffix('.')?;
    (!sub.is_empty() && !sub.contains('.')).then_some(sub)
}

fn login_transform(base_domain: Option<String>) -> LoginTransform {
    Arc::new(move |headers: &HeaderMap, body: Value| {
        let host = headers
            .get("host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let from_host = base_domain
            .as_deref()
            .and_then(|base| tenant_from_host(host, base))
            .map(str::to_string);
        let tenant = from_host
            .or_else(|| body["tenant"].as_str().map(|s| s.trim().to_lowercase()))
            .filter(|t| !t.is_empty())
            .ok_or_else(|| ApiError::bad_request("tenant is required"))?;
        let field = |name: &str| {
            body[name]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| ApiError::bad_request(format!("{name} is required")))
        };
        Ok(json!({
            "tenant": tenant,
            "email": field("email")?,
            "password": field("password")?,
        }))
    })
}

pub fn build(cfg: &CrmConfig, store: Arc<dyn SessionStore>) -> Router {
    let bff_config = BffConfig {
        cookie_name: COOKIE_NAME.into(),
        cookie_secure: cfg.cookie_secure,
        session_ttl: Duration::from_secs(cfg.session_ttl_secs),
        upstream_base_url: cfg.identity_url.clone(),
        upstreams: HashMap::from([
            ("candidate".to_string(), cfg.candidate_url.clone()),
            ("recruitment".to_string(), cfg.recruitment_url.clone()),
        ]),
        allow: allowlist(),
        cors_origins: cfg.cors_origins.clone(),
        login_transform: Some(login_transform(cfg.base_domain.clone())),
    };
    let auth = Arc::new(HttpUpstreamAuth::new(
        &cfg.identity_url,
        "/auth/login",
        Some("/auth/refresh"),
        Some("/auth/logout"),
    ));
    let doc = bff_core::openapi("CRM BFF");
    bff_core::router(Bff::new(bff_config, store, auth))
        .merge(platform_core::health::router())
        .route("/openapi.json", get(move || async move { Json(doc) }))
}

#[cfg(test)]
mod tests {
    use super::tenant_from_host;

    #[test]
    fn extracts_the_tenant_subdomain() {
        assert_eq!(tenant_from_host("acme.crm.test", "crm.test"), Some("acme"));
        assert_eq!(
            tenant_from_host("acme.crm.test:8080", "crm.test"),
            Some("acme")
        );
    }

    #[test]
    fn rejects_hosts_outside_the_base_domain() {
        for host in [
            "crm.test",
            "evil.com",
            "a.b.crm.test",
            "acme.crm.test.evil.com",
            "xcrm.test",
            "",
        ] {
            assert_eq!(tenant_from_host(host, "crm.test"), None, "{host}");
        }
    }
}
