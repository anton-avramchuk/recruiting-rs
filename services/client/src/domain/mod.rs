//! Предметная область заказчиков: чистая логика без HTTP и БД.
mod client;
mod contact;
mod error;
mod position;

pub use client::{Client, ClientPatch, NewClient, name_key};
pub use contact::{Contact, ContactPatch, NewContact};
pub use error::DomainError;
pub use position::Position;

/// Свободный текст: пробелы по краям убираются, пустое значение - «не указано».
pub(crate) fn text(raw: Option<String>) -> Option<String> {
    raw.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Обязательное название: после обрезки не пусто и не длиннее `max` символов.
pub(crate) fn required_name(
    field: &'static str,
    raw: &str,
    max: usize,
) -> Result<String, DomainError> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(DomainError::invalid(field, "не может быть пустым"));
    }
    if name.chars().count() > max {
        return Err(DomainError::invalid(
            field,
            format!("не длиннее {max} символов"),
        ));
    }
    Ok(name)
}
