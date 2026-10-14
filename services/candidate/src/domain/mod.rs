//! Предметная область кандидата: чистая логика без HTTP и БД.
mod contacts;
mod error;
mod history;
mod matcher;
mod person;

pub use contacts::{Telegram, WhatsApp};
pub use error::DomainError;
pub use history::{Education, Employment, experience_years};
pub use matcher::{Duplicate, MatchedBy, PersonKeys, Probe, find_duplicate};
pub use person::{ContactsInput, NewPerson, Person, PersonRecord, ProfilePatch, normalize_skills};
