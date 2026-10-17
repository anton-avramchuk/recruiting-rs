//! Серверная часть грида: белый список полей (схема), проверка запроса по схеме и лимитам и
//! безопасная сборка SQL. Значения клиента попадают в запрос только параметрами.
mod build;
mod fetch;

pub use build::{Bind, Built, build};
pub use fetch::{FetchError, fetch_page};
pub use grid_contracts as contracts;

use platform_core::error::ApiError;

/// Тип столбца: по нему значения фильтра приводятся к типу SQL-параметра.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    Text,
    Int,
    Bool,
    Uuid,
    Timestamp,
    Date,
}

/// Поле грида, доступное клиенту.
#[derive(Debug, Clone, Copy)]
pub struct GridColumn {
    /// Публичное имя: по нему клиент сортирует и фильтрует.
    pub name: &'static str,
    /// SQL-выражение столбца. Берётся только из кода сервиса, не от клиента.
    pub sql: &'static str,
    pub ty: ColumnType,
    pub sortable: bool,
    pub filterable: bool,
}

impl GridColumn {
    pub const fn new(name: &'static str, sql: &'static str, ty: ColumnType) -> Self {
        Self {
            name,
            sql,
            ty,
            sortable: true,
            filterable: true,
        }
    }

    pub const fn no_sort(mut self) -> Self {
        self.sortable = false;
        self
    }

    pub const fn no_filter(mut self) -> Self {
        self.filterable = false;
        self
    }
}

/// Источник грида: таблица или представление и допустимые поля.
#[derive(Debug, Clone, Copy)]
pub struct GridSchema {
    /// Таблица или представление (из кода).
    pub source: &'static str,
    /// Публичное имя ключевого столбца: добавляется к сортировке для стабильного порядка.
    pub key: &'static str,
    pub columns: &'static [GridColumn],
    /// Наибольший размер страницы.
    pub max_page_size: u32,
}

/// Размер страницы по умолчанию для схем без своего предела.
pub const MAX_PAGE_SIZE: u32 = 100;
/// Наибольшая вложенность групп фильтра (корень - 1).
pub const MAX_DEPTH: usize = 4;
/// Наибольшее число условий-листьев.
pub const MAX_CONDITIONS: usize = 32;
/// Наибольшее число ключей сортировки.
pub const MAX_SORT_KEYS: usize = 3;
/// Наибольшее число значений в перечне оператора `in`.
pub const MAX_IN_VALUES: usize = 200;
/// Наибольшая длина значения условия, символов.
pub const MAX_VALUE_LEN: usize = 200;

/// Запрос не прошёл проверку: какой параметр и почему.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{param}: {message}")]
pub struct GridError {
    pub param: String,
    pub message: String,
}

impl GridError {
    pub fn new(param: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            param: param.into(),
            message: message.into(),
        }
    }
}

impl From<grid_contracts::QueryError> for GridError {
    fn from(e: grid_contracts::QueryError) -> Self {
        Self::new(e.param, e.message)
    }
}

impl From<GridError> for ApiError {
    fn from(e: GridError) -> Self {
        ApiError::bad_request(e.message).with_extension("field", serde_json::Value::String(e.param))
    }
}

impl GridSchema {
    pub(crate) fn column(&self, name: &str) -> Option<&GridColumn> {
        let wanted = grid_contracts::normalize_field(name);
        self.columns
            .iter()
            .find(|c| grid_contracts::normalize_field(c.name) == wanted)
    }
}
