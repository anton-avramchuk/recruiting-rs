//! Доступ к PostgreSQL: пул, миграции, транзакции в контексте тенанта.
mod like;
mod pool;
mod tx;

pub use like::{contains_pattern, escape_like};
pub use pool::{connect, migrate};
pub use tx::TenantTx;

#[cfg(feature = "test-support")]
pub mod testing;
