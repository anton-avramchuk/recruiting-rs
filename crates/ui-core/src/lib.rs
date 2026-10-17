//! ui-core: логика интерфейса без Leptos и без браузера. Проверяется нативными юнит-тестами;
//! крейты `ui`, `ui-data`, `ui-shell` и приложения строятся на ней.
mod encode;
pub mod grid;
pub mod mutation;
pub mod paging;
pub mod reply;

pub use encode::encode_query_value;
pub use grid::{
    FilterSpec, GridUrl, PAGE_SIZES, RowActions, closes_dialog, confirm_question, is_interactive,
    page_info,
};
pub use reply::{Reply, classify, classify_empty, typed};
