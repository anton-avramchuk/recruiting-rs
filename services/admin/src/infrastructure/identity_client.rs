use std::time::Duration;

use async_trait::async_trait;
use platform_core::tenant::TenantId;
use serde_json::json;

use crate::application::provisioning::{OwnerSpec, ProvisionError, Provisioner};

/// Клиент internal-API сервиса identity.
pub struct HttpProvisioner {
    client: reqwest::Client,
    base_url: String,
    service_token: String,
}

impl HttpProvisioner {
    pub fn new(base_url: impl Into<String>, service_token: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("build http client"),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            service_token: service_token.into(),
        }
    }
}

#[async_trait]
impl Provisioner for HttpProvisioner {
    async fn provision_owner(
        &self,
        tenant: TenantId,
        owner: &OwnerSpec,
    ) -> Result<(), ProvisionError> {
        let res = self
            .client
            .post(format!("{}/internal/tenants/{tenant}/owner", self.base_url))
            .bearer_auth(&self.service_token)
            .json(&json!({
                "email": owner.email,
                "password": owner.password,
                "display_name": owner.display_name,
            }))
            .send()
            .await
            .map_err(|e| ProvisionError(e.to_string()))?;
        if res.status().is_success() {
            Ok(())
        } else {
            Err(ProvisionError(format!(
                "identity responded {}",
                res.status()
            )))
        }
    }
}
