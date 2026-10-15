/// Ошибка домена: неверное значение поля либо конфликт с текущим состоянием.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("{field}: {reason}")]
    Invalid { field: &'static str, reason: String },
    #[error("{0}")]
    Conflict(String),
}

impl DomainError {
    pub fn invalid(field: &'static str, reason: impl Into<String>) -> Self {
        Self::Invalid {
            field,
            reason: reason.into(),
        }
    }

    pub fn conflict(reason: impl Into<String>) -> Self {
        Self::Conflict(reason.into())
    }
}
