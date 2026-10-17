//! Грид перенесён в `ui` (компонент) и `ui-core` (состояние); модуль оставлен как фасад для приложений.
pub use ui::{Column, ColumnFilter, DebouncedText, Grid, RowExtra, RowRemove};
pub use ui_core::grid::{
    FilterSpec, GridUrl, PAGE_SIZES, RowActions, closes_dialog, confirm_question, grid_path,
    is_interactive, page_info, state,
};
