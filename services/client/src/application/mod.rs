//! Сценарии сервиса: транзакции в контексте тенанта и правила между агрегатами.
mod clients;
mod feed;
mod positions;

use uuid::Uuid;

pub use clients::{ClientDetail, ClientService};
pub use feed::FeedService;
pub use positions::PositionService;

use crate::domain::DomainError;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("not found")]
    NotFound,
    /// Заказчик с таким названием уже есть.
    #[error("client with this name exists")]
    DuplicateClient { id: Uuid, name: String },
    /// Должность с таким названием уже есть.
    #[error("position with this name exists")]
    DuplicatePosition { id: Uuid, name: String },
    /// Должность назначена контактам: удалить нельзя.
    #[error("position is used by {0} contact(s)")]
    PositionInUse(i64),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}
