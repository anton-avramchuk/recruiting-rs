//! Логика грида без разметки: состояние в адресе, действия строки.
pub mod actions;
pub mod filter_ops;
pub mod path;
pub mod selection;
pub mod state;

pub use actions::{RowActions, closes_dialog, confirm_question, is_interactive};
pub use filter_ops::{and_condition, take_eq};
pub use path::grid_path;
pub use selection::{Selection, clamp_cursor, move_cursor};
pub use state::{FilterSpec, GridUrl, PAGE_SIZES, page_info};
