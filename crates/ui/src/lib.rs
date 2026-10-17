//! ui: контролы Leptos. Логика (без разметки) лежит в `ui-core`, состояние данных экранов - в `ui-data`.
/// Токены и стили общих контролов.
pub const CSS: &str = include_str!("../style/ui.css");

mod dialog;
mod forms;
mod grid;
mod indicators;
mod navigation;
mod pager;

pub use dialog::{ConfirmDialog, Dialog, Drawer};
pub use forms::{Checkbox, DatePicker, Dropdown, Field, NumberBox, TextBox, TextField, Textarea};
pub use grid::{Column, ColumnFilter, DebouncedText, Grid, RowExtra, RowRemove};
pub use indicators::{
    Avatar, Badge, BadgeTone, Card, Chip, EmptyState, Notice, NoticeKind, PageHeader,
};
pub use navigation::{Tab, Tabs, Toolbar};
pub use pager::Pager;
