use platform_auth::AuthUser;
use platform_core::{error::ApiError, tenant::TenantId};

use crate::{application::CommentError, domain::DomainError};

/// Тенант берётся только из токена, никогда из запроса.
pub(super) fn tenant_of(user: &AuthUser) -> Result<TenantId, ApiError> {
    user.claims.tid.ok_or_else(ApiError::forbidden)
}

impl From<CommentError> for ApiError {
    fn from(e: CommentError) -> Self {
        match e {
            CommentError::Domain(DomainError::Invalid { field, reason }) => {
                ApiError::bad_request(reason).with_extension("field", field.into())
            }
            CommentError::NotFound => ApiError::not_found(),
            CommentError::Forbidden => ApiError::forbidden(),
            CommentError::BadQuery(m) => ApiError::bad_request(m),
            CommentError::Db(e) => ApiError::internal(&e),
        }
    }
}
