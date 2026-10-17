//! Сценарии воронки: транзакции, права на уровне данных (RLS) и синхронизация людей.
pub mod analytics;
pub mod applications;
mod client_feed;
mod client_sync;
mod feed;
mod sync;
mod vacancies;

use chrono::{DateTime, NaiveDate, Utc};

pub use analytics::{Analytics, AnalyticsService, DEFAULT_DAYS};
pub use applications::{
    ApplicationDetail, ApplicationPage, ApplicationService, BatchOutcome, BulkAction, BulkOutcome,
    ListQuery, MAX_PAGE_SIZE, Overview,
};
pub use client_feed::{ClientFeed, HttpClientFeed};
pub use client_sync::ClientsSync;
pub use feed::{FeedError, HttpPersonFeed, PersonFeed};
pub use sync::PeopleSync;
pub use vacancies::VacancyService;

use crate::{domain::DomainError, infrastructure::StoreError};

#[derive(Debug, thiserror::Error)]
pub enum FunnelError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("not found")]
    NotFound,
    #[error("application already exists")]
    Duplicate,
    /// Сервис заказчиков недоступен, а нужного заказчика в проекции ещё нет.
    #[error("clients are unavailable")]
    ClientsUnavailable,
    #[error("{0}")]
    BadQuery(String),
    #[error(transparent)]
    Grid(#[from] platform_grid::GridError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<platform_grid::FetchError> for FunnelError {
    fn from(e: platform_grid::FetchError) -> Self {
        match e {
            platform_grid::FetchError::Grid(e) => FunnelError::Grid(e),
            platform_grid::FetchError::Db(e) => FunnelError::Db(e),
        }
    }
}

/// Источник «сейчас»: фиксируется в тестах.
#[derive(Clone, Copy)]
pub enum Clock {
    System,
    Fixed(DateTime<Utc>),
}

impl Clock {
    pub fn now(self) -> DateTime<Utc> {
        match self {
            Clock::System => Utc::now(),
            Clock::Fixed(at) => at,
        }
    }

    pub fn today(self) -> NaiveDate {
        self.now().date_naive()
    }
}
