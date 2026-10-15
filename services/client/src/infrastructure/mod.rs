//! Хранение заказчиков. Все функции работают на соединении в контексте тенанта (RLS),
//! поэтому фильтр по `tenant_id` в запросах не нужен.
pub mod changes;
pub mod clients;
pub mod contacts;
pub mod positions;

/// Нарушение уникальности (код PostgreSQL 23505).
pub fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23505")
}

/// Нарушение внешнего ключа (код PostgreSQL 23503).
pub fn is_foreign_key_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23503")
}
