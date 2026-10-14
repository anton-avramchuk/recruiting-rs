//! Справочник тенантов: HTTP-клиент сервиса admin и кэш с TTL.
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use platform_core::tenant::TenantId;
use reqwest::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use crate::application::tenants::{DirectoryError, TenantDirectory, TenantInfo, TenantStatus};

#[derive(Deserialize)]
struct TenantDto {
    id: Uuid,
    status: String,
}

impl TenantDto {
    /// Тенанты в статусах `provisioning`/`failed` для identity не существуют.
    fn into_info(self) -> Option<TenantInfo> {
        let status = match self.status.as_str() {
            "active" => TenantStatus::Active,
            "suspended" => TenantStatus::Suspended,
            _ => return None,
        };
        Some(TenantInfo {
            id: TenantId(self.id),
            status,
        })
    }
}

pub struct HttpTenantDirectory {
    client: reqwest::Client,
    base_url: String,
    service_token: String,
}

impl HttpTenantDirectory {
    pub fn new(base_url: impl Into<String>, service_token: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("build http client"),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            service_token: service_token.into(),
        }
    }

    async fn fetch(&self, path: &str) -> Result<Option<TenantInfo>, DirectoryError> {
        let res = self
            .client
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(&self.service_token)
            .send()
            .await
            .map_err(|e| DirectoryError(e.to_string()))?;
        match res.status() {
            StatusCode::NOT_FOUND => Ok(None),
            s if s.is_success() => {
                let dto: TenantDto = res
                    .json()
                    .await
                    .map_err(|e| DirectoryError(e.to_string()))?;
                Ok(dto.into_info())
            }
            s => Err(DirectoryError(format!("admin responded {s}"))),
        }
    }
}

#[async_trait]
impl TenantDirectory for HttpTenantDirectory {
    async fn resolve(&self, slug: &str) -> Result<Option<TenantInfo>, DirectoryError> {
        self.fetch(&format!("/internal/tenants/by-slug/{slug}"))
            .await
    }

    async fn status(&self, id: TenantId) -> Result<Option<TenantStatus>, DirectoryError> {
        Ok(self
            .fetch(&format!("/internal/tenants/{id}"))
            .await?
            .map(|t| t.status))
    }
}

/// Кэширует положительные ответы на `ttl`. Ошибки и «не найден» не кэшируются:
/// только что созданный тенант должен стать доступным сразу.
/// Приостановка вступает в силу с задержкой не более `ttl`.
pub struct CachedDirectory<D> {
    inner: D,
    ttl: Duration,
    by_slug: Mutex<HashMap<String, (Instant, TenantInfo)>>,
    by_id: Mutex<HashMap<TenantId, (Instant, TenantStatus)>>,
}

impl<D> CachedDirectory<D> {
    pub fn new(inner: D, ttl: Duration) -> Self {
        Self {
            inner,
            ttl,
            by_slug: Mutex::default(),
            by_id: Mutex::default(),
        }
    }
}

#[async_trait]
impl<D: TenantDirectory> TenantDirectory for CachedDirectory<D> {
    async fn resolve(&self, slug: &str) -> Result<Option<TenantInfo>, DirectoryError> {
        let hit = {
            let cache = self.by_slug.lock().expect("cache lock");
            cache
                .get(slug)
                .filter(|(at, _)| at.elapsed() < self.ttl)
                .map(|(_, info)| *info)
        };
        if hit.is_some() {
            return Ok(hit);
        }
        let fresh = self.inner.resolve(slug).await?;
        if let Some(info) = fresh {
            self.by_slug
                .lock()
                .expect("cache lock")
                .insert(slug.to_string(), (Instant::now(), info));
        }
        Ok(fresh)
    }

    async fn status(&self, id: TenantId) -> Result<Option<TenantStatus>, DirectoryError> {
        let hit = {
            let cache = self.by_id.lock().expect("cache lock");
            cache
                .get(&id)
                .filter(|(at, _)| at.elapsed() < self.ttl)
                .map(|(_, s)| *s)
        };
        if hit.is_some() {
            return Ok(hit);
        }
        let fresh = self.inner.status(id).await?;
        if let Some(status) = fresh {
            self.by_id
                .lock()
                .expect("cache lock")
                .insert(id, (Instant::now(), status));
        }
        Ok(fresh)
    }
}
