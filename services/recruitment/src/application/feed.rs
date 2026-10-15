//! Источник ленты изменений кандидатов (сервис candidate).
use std::time::Duration;

use async_trait::async_trait;
use candidate_contracts::PersonChangesPage;
use platform_core::tenant::TenantId;

#[derive(Debug, thiserror::Error)]
pub enum FeedError {
    #[error("person feed unavailable: {0}")]
    Unavailable(String),
}

#[async_trait]
pub trait PersonFeed: Send + Sync {
    /// Изменения кандидатов организации после позиции `after`, не больше `limit` записей.
    async fn changes(
        &self,
        tenant: TenantId,
        after: i64,
        limit: i64,
    ) -> Result<PersonChangesPage, FeedError>;
}

/// Лента по HTTP: внутренний эндпоинт candidate с сервисным токеном.
pub struct HttpPersonFeed {
    client: reqwest::Client,
    base_url: String,
    service_token: String,
}

impl HttpPersonFeed {
    pub fn new(base_url: &str, service_token: &str) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
            base_url: base_url.trim_end_matches('/').to_string(),
            service_token: service_token.to_string(),
        }
    }
}

#[async_trait]
impl PersonFeed for HttpPersonFeed {
    async fn changes(
        &self,
        tenant: TenantId,
        after: i64,
        limit: i64,
    ) -> Result<PersonChangesPage, FeedError> {
        let url = format!(
            "{}/internal/tenants/{}/person-changes?after={after}&limit={limit}",
            self.base_url, tenant.0
        );
        let unavailable = |e: reqwest::Error| FeedError::Unavailable(e.to_string());
        self.client
            .get(url)
            .bearer_auth(&self.service_token)
            .send()
            .await
            .map_err(unavailable)?
            .error_for_status()
            .map_err(unavailable)?
            .json()
            .await
            .map_err(unavailable)
    }
}
