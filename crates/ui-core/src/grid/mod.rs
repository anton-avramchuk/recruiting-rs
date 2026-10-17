//! Логика грида без разметки: состояние в адресе, действия строки.
pub mod actions;
pub mod state;

pub use actions::{RowActions, closes_dialog, confirm_question, is_interactive};
pub use state::{FilterSpec, GridUrl, PAGE_SIZES, page_info};
