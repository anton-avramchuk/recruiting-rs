//! Источник ленты изменений заказчиков (сервис client).
use std::time::Duration;

use async_trait::async_trait;
use client_contracts::ClientChangesPage;
use platform_core::tenant::TenantId;

use super::FeedError;

#[async_trait]
pub trait ClientFeed: Send + Sync {
    /// Изменения заказчиков организации после позиции `after`, не больше `limit` записей.
    async fn changes(
        &self,
        tenant: TenantId,
        after: i64,
        limit: i64,
    ) -> Result<ClientChangesPage, FeedError>;
}

/// Лента по HTTP: внутренний эндпоинт client с сервисным токеном.
pub struct HttpClientFeed {
    client: reqwest::Client,
    base_url: String,
    service_token: String,
}

impl HttpClientFeed {
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
impl ClientFeed for HttpClientFeed {
    async fn changes(
        &self,
        tenant: TenantId,
        after: i64,
        limit: i64,
    ) -> Result<ClientChangesPage, FeedError> {
        let url = format!(
            "{}/internal/tenants/{}/client-changes?after={after}&limit={limit}",
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
