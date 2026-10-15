use platform_auth::AuthUser;
use platform_core::{error::ApiError, tenant::TenantId};
use serde_json::json;

use crate::{application::ClientError, domain::DomainError};

/// Тенант берётся только из токена, никогда из запроса.
pub(super) fn tenant_of(user: &AuthUser) -> Result<TenantId, ApiError> {
    user.claims.tid.ok_or_else(ApiError::forbidden)
}

impl From<ClientError> for ApiError {
    fn from(e: ClientError) -> Self {
        match e {
            ClientError::Domain(DomainError::Invalid { field, reason }) => {
                ApiError::bad_request(reason).with_extension("field", field.into())
            }
            ClientError::NotFound => ApiError::not_found(),
            ClientError::DuplicateClient { id, name } => ApiError::conflict("client exists")
                .with_extension("duplicate", json!({"id": id, "name": name})),
            ClientError::DuplicatePosition { id, name } => ApiError::conflict("position exists")
                .with_extension("duplicate", json!({"id": id, "name": name})),
            ClientError::PositionInUse(n) => {
                ApiError::conflict(format!("position is assigned to {n} contact(s)"))
                    .with_extension("field", "position".into())
            }
            ClientError::Db(e) => ApiError::internal(&e),
        }
    }
}
