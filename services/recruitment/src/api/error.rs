use platform_core::error::ApiError;

use crate::{application::FunnelError, domain::DomainError};

impl From<FunnelError> for ApiError {
    fn from(e: FunnelError) -> Self {
        match e {
            FunnelError::Domain(DomainError::Invalid { field, reason }) => {
                ApiError::bad_request(reason).with_extension("field", field.into())
            }
            FunnelError::Domain(DomainError::Conflict(reason)) => ApiError::conflict(reason),
            FunnelError::NotFound => ApiError::not_found(),
            FunnelError::Duplicate => ApiError::conflict("application already exists"),
            FunnelError::ClientsUnavailable => ApiError::new(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Service Unavailable",
            )
            .with_detail("сервис заказчиков временно недоступен, попробуйте позже"),
            FunnelError::BadQuery(m) => ApiError::bad_request(m),
            FunnelError::Grid(e) => e.into(),
            FunnelError::Db(e) => ApiError::internal(&e),
            FunnelError::Store(e) => ApiError::internal(&e),
        }
    }
}
