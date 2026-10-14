//! Сценарии использования candidate.
pub mod persons;

pub use persons::{
    Clock, ContactsPatch, ListQuery, Page, PersonError, PersonService, UpdatePerson,
};
