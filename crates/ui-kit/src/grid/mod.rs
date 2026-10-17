//! Общий грид: таблица с серверной пагинацией, сортировкой, фильтрами столбцов и поиском.
//! Состояние хранится в адресной строке (`state`), данные приходят со страницы сервера.
mod actions;
pub mod state;
mod view;

pub use actions::{RowActions, closes_dialog, confirm_question, is_interactive};
pub use state::{FilterSpec, GridUrl, PAGE_SIZES, page_info};
pub use view::{Column, ColumnFilter, DebouncedText, Grid, RowRemove};
