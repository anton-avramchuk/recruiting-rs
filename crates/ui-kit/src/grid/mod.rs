//! Общий грид: таблица с серверной пагинацией, сортировкой, фильтрами столбцов и поиском.
//! Состояние хранится в адресной строке (`ui_core::grid::state`), данные приходят со страницы сервера.
mod view;

pub use ui_core::grid::{
    FilterSpec, GridUrl, PAGE_SIZES, RowActions, closes_dialog, confirm_question, is_interactive,
    page_info, state,
};
pub use view::{Column, ColumnFilter, DebouncedText, Grid, RowRemove};
