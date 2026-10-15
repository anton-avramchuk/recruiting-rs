//! Сценарии использования candidate.
pub mod persons;

pub use persons::{
    Clock, ContactsPatch, ImportItem, ImportOutcome, ListQuery, Page, PersonError, PersonService,
    UpdatePerson,
};
