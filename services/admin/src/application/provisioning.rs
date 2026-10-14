use async_trait::async_trait;
use platform_core::tenant::TenantId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerSpec {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

#[derive(Debug, thiserror::Error)]
#[error("owner provisioning failed: {0}")]
pub struct ProvisionError(pub String);

/// Порт: создание первого владельца тенанта в сервисе identity.
/// Реализации обязаны быть идемпотентными по `tenant`.
#[async_trait]
pub trait Provisioner: Send + Sync {
    async fn provision_owner(
        &self,
        tenant: TenantId,
        owner: &OwnerSpec,
    ) -> Result<(), ProvisionError>;
}
