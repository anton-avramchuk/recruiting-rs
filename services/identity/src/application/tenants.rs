use async_trait::async_trait;
use platform_core::tenant::TenantId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantStatus {
    Active,
    Suspended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantInfo {
    pub id: TenantId,
    pub status: TenantStatus,
}

#[derive(Debug, thiserror::Error)]
#[error("tenant directory unavailable: {0}")]
pub struct DirectoryError(pub String);

/// Порт: справочник тенантов (реализация — клиент сервиса admin).
#[async_trait]
pub trait TenantDirectory: Send + Sync {
    async fn resolve(&self, slug: &str) -> Result<Option<TenantInfo>, DirectoryError>;
    async fn status(&self, id: TenantId) -> Result<Option<TenantStatus>, DirectoryError>;
}
