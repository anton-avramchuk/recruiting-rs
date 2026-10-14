//! Предметная область admin: тенант и его жизненный цикл.
mod tenant;

pub use tenant::{DomainError, Slug, Tenant, TenantStatus};
