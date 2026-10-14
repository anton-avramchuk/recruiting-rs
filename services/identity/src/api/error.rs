use axum::http::StatusCode;
use platform_core::error::ApiError;

use crate::application::auth::AuthFailure;

impl From<AuthFailure> for ApiError {
    fn from(f: AuthFailure) -> Self {
        match f {
            AuthFailure::InvalidCredentials => ApiError::unauthorized(),
            AuthFailure::TenantSuspended => {
                ApiError::new(StatusCode::FORBIDDEN, "Forbidden").with_detail("tenant suspended")
            }
            AuthFailure::Directory(e) => ApiError::internal(&e),
            AuthFailure::Db(e) => ApiError::internal(&e),
            AuthFailure::Internal(m) => ApiError::internal(&std::io::Error::other(m)),
        }
    }
}
