//! Предметная область комментариев: чистая логика без HTTP и БД.
mod comment;
mod error;

pub use comment::{Comment, NewComment};
pub use error::DomainError;
