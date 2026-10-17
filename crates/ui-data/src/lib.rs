//! ui-data: состояние данных экранов на сигналах - чтение (`Query`), команды (`Mutation`) и
//! сетка записей (`EntityGrid`). Логика состояний без реактивности проверяется в `ui-core`.
mod entity_grid;
mod mutation;
mod query;

pub use entity_grid::{EntityGrid, EntityRemove};
pub use mutation::{Mutation, MutationConfig};
pub use query::{Query, QueryHandle, use_query};
