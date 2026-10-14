use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

/// Результат успешной аутентификации в бэкенде.
#[derive(Debug, Clone, Deserialize)]
pub struct Grant {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_in: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum UpstreamError {
    /// Бэкенд отказал (неверные данные, тенант приостановлен, токен отозван).
    #[error("upstream rejected the request with {status}")]
    Rejected { status: u16 },
    #[error("upstream unavailable: {0}")]
    Unavailable(String),
}

/// Аутентификация BFF в бэкенде: вход, обновление и отзыв токенов.
#[async_trait]
pub trait UpstreamAuth: Send + Sync {
    async fn login(&self, credentials: Value) -> Result<Grant, UpstreamError>;
    /// `None` в ответ на `refresh`, если слой не поддерживает обновление.
    async fn refresh(&self, refresh_token: &str) -> Result<Grant, UpstreamError>;
    async fn logout(&self, refresh_token: &str) -> Result<(), UpstreamError>;
}

pub struct HttpUpstreamAuth {
    client: reqwest::Client,
    base_url: String,
    login_path: String,
    refresh_path: Option<String>,
    logout_path: Option<String>,
}

impl HttpUpstreamAuth {
    pub fn new(
        base_url: &str,
        login_path: &str,
        refresh_path: Option<&str>,
        logout_path: Option<&str>,
    ) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("build http client"),
            base_url: base_url.trim_end_matches('/').to_string(),
            login_path: login_path.into(),
            refresh_path: refresh_path.map(Into::into),
            logout_path: logout_path.map(Into::into),
        }
    }

    async fn grant(&self, path: &str, body: &Value) -> Result<Grant, UpstreamError> {
        let res = self
            .client
            .post(format!("{}{path}", self.base_url))
            .json(body)
            .send()
            .await
            .map_err(|e| UpstreamError::Unavailable(e.to_string()))?;
        let status = res.status();
        if status.is_success() {
            res.json::<Grant>()
                .await
                .map_err(|e| UpstreamError::Unavailable(e.to_string()))
        } else if status.is_client_error() {
            Err(UpstreamError::Rejected {
                status: status.as_u16(),
            })
        } else {
            Err(UpstreamError::Unavailable(format!(
                "upstream answered {status}"
            )))
        }
    }
}

#[async_trait]
impl UpstreamAuth for HttpUpstreamAuth {
    async fn login(&self, credentials: Value) -> Result<Grant, UpstreamError> {
        self.grant(&self.login_path, &credentials).await
    }

    async fn refresh(&self, refresh_token: &str) -> Result<Grant, UpstreamError> {
        match &self.refresh_path {
            Some(path) => {
                self.grant(path, &json!({ "refresh_token": refresh_token }))
                    .await
            }
            None => Err(UpstreamError::Rejected { status: 401 }),
        }
    }

    async fn logout(&self, refresh_token: &str) -> Result<(), UpstreamError> {
        let Some(path) = &self.logout_path else {
            return Ok(());
        };
        self.client
            .post(format!("{}{path}", self.base_url))
            .json(&json!({ "refresh_token": refresh_token }))
            .send()
            .await
            .map_err(|e| UpstreamError::Unavailable(e.to_string()))?;
        Ok(())
    }
}
