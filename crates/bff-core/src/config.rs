use std::{sync::Arc, time::Duration};

use axum::http::{HeaderMap, Method};
use platform_core::error::ApiError;
use serde_json::Value;

/// Преобразует тело входа до отправки в бэкенд (например, берёт тенанта из поддомена).
pub type LoginTransform = Arc<dyn Fn(&HeaderMap, Value) -> Result<Value, ApiError> + Send + Sync>;

/// Правило allowlist: какие маршруты бэкенда доступны фронтенду через BFF.
#[derive(Debug, Clone)]
pub struct AllowRule {
    method: Option<Method>,
    prefix: String,
}

impl AllowRule {
    pub fn any(prefix: &str) -> Self {
        Self {
            method: None,
            prefix: prefix.into(),
        }
    }

    pub fn only(method: Method, prefix: &str) -> Self {
        Self {
            method: Some(method),
            prefix: prefix.into(),
        }
    }

    /// Совпадение по границе сегмента: `/users` разрешает `/users` и `/users/1`, но не `/users-x`.
    pub fn allows(&self, method: &Method, path: &str) -> bool {
        self.method.as_ref().is_none_or(|m| m == method)
            && (path == self.prefix
                || path
                    .strip_prefix(&self.prefix)
                    .is_some_and(|rest| rest.starts_with('/')))
    }
}

#[derive(Clone)]
pub struct BffConfig {
    /// Имя cookie сессии; у слоёв различается (`crm_session`, `admin_session`).
    pub cookie_name: String,
    pub cookie_secure: bool,
    pub session_ttl: Duration,
    pub upstream_base_url: String,
    pub allow: Vec<AllowRule>,
    pub login_transform: Option<LoginTransform>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_rule_matches_on_segment_boundaries() {
        let rule = AllowRule::any("/users");
        assert!(rule.allows(&Method::GET, "/users"));
        assert!(rule.allows(&Method::POST, "/users/42"));
        assert!(!rule.allows(&Method::GET, "/users-admin"));
        assert!(!rule.allows(&Method::GET, "/usersx/1"));
        assert!(!rule.allows(&Method::GET, "/"));
    }

    #[test]
    fn allow_rule_can_restrict_the_method() {
        let rule = AllowRule::only(Method::GET, "/me");
        assert!(rule.allows(&Method::GET, "/me"));
        assert!(!rule.allows(&Method::DELETE, "/me"));
    }
}
