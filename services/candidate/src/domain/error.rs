/// Ошибка проверки: поле называется явно, чтобы интерфейс мог показать её у нужного места.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("{field}: {reason}")]
    Invalid { field: &'static str, reason: String },
}

impl DomainError {
    pub fn invalid(field: &'static str, reason: impl Into<String>) -> Self {
        Self::Invalid {
            field,
            reason: reason.into(),
        }
    }
}
