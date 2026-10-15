//! Предметная область воронки: чистая логика без HTTP и БД.
mod application;
mod error;
mod vacancy;

pub use application::{Application, EventKind, NewEvent};
pub use error::DomainError;
pub use vacancy::{NewVacancy, Vacancy, VacancyPatch};
