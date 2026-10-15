//! Раздел «Заказчики»: список, карточка с контактами и справочник должностей.
pub mod card;
pub mod list;
pub mod positions;
pub mod server;

/// Пустая строка — «не указано».
pub fn opt(raw: &str) -> Option<String> {
    Some(raw.trim().to_string()).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::opt;

    #[test]
    fn blank_text_is_not_set() {
        assert_eq!(opt("  "), None);
        assert_eq!(opt(" Acme "), Some("Acme".into()));
    }
}
