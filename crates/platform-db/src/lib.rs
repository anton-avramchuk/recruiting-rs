//! Доступ к PostgreSQL: пул, миграции, транзакции в контексте тенанта.
mod pool;
mod tx;

pub use pool::{connect, migrate};
pub use tx::TenantTx;

#[cfg(feature = "test-support")]
pub mod testing;
