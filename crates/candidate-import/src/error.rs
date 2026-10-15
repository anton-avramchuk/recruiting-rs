use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    /// В файле нет заголовка и хотя бы одной строки данных.
    NoData,
    /// Файл не удалось разобрать как заявленный формат.
    Unreadable(String),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::NoData => write!(f, "в файле нет строк с данными"),
            ImportError::Unreadable(why) => write!(f, "не удалось разобрать файл: {why}"),
        }
    }
}

impl std::error::Error for ImportError {}
