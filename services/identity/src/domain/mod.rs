//! Предметная область identity: без зависимостей от HTTP и БД.
mod user;

pub use user::{DomainError, Email, User, UserStatus};
