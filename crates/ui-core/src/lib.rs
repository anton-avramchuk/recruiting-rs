//! ui-core: логика интерфейса без Leptos и без браузера. Проверяется нативными юнит-тестами;
//! крейты `ui`, `ui-data`, `ui-shell` и приложения строятся на ней.
mod encode;
pub mod form;
pub mod format;
pub mod grid;
pub mod mutation;
pub mod paging;
pub mod policy;
pub mod reply;
pub mod tabs;

pub use encode::encode_query_value;
pub use grid::{
    FilterSpec, GridUrl, PAGE_SIZES, RowActions, and_condition, closes_dialog, confirm_question,
    grid_path, is_interactive, page_info, take_eq,
};
pub use reply::{Reply, classify, classify_empty, typed};
