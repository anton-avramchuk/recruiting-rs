//! Предметная область identity: без зависимостей от HTTP и БД.
mod role;
mod user;

pub use role::{BUILT_IN, BuiltIn, CustomRole};
pub use user::{DomainError, Email, User, UserStatus};
