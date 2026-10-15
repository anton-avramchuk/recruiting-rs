pub mod applications;
pub mod people;
pub mod vacancies;

use crate::domain::DomainError;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    /// В базе лежит значение, не проходящее проверки домена.
    #[error("stored data is inconsistent: {0}")]
    Corrupt(#[from] DomainError),
}

/// Нарушение уникальности (код PostgreSQL 23505).
pub fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23505")
}
