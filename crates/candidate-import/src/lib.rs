//! Разбор файлов импорта кандидатов: таблицы (CSV, XLSX), PDF-резюме и текст резюме.
//!
//! Чистая логика без ввода-вывода: работает и в браузере (wasm), и в тестах на образцах.
pub mod columns;
pub mod delimited;
mod error;
pub mod pdf;
pub mod resume;
pub mod rows;
pub mod xlsx;

pub use error::ImportError;

/// Таблица: строки ячеек; первая строка — заголовок.
pub type Grid = Vec<Vec<String>>;
