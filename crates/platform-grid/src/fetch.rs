//! Выполнение запроса грида: счётчик и страница в одном соединении.
use grid_contracts::{GridPage, GridQuery};
use sqlx::{FromRow, PgConnection, postgres::PgRow};

use crate::{Bind, GridError, GridSchema, build};

/// Ошибка выполнения: некорректный запрос либо сбой БД.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error(transparent)]
    Grid(#[from] GridError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

macro_rules! bind_all {
    ($query:expr, $binds:expr) => {{
        let mut query = $query;
        for bind in $binds {
            query = match bind {
                Bind::Text(v) => query.bind(v),
                Bind::Int(v) => query.bind(v),
                Bind::Bool(v) => query.bind(v),
                Bind::Uuid(v) => query.bind(v),
                Bind::Timestamp(v) => query.bind(v),
                Bind::Date(v) => query.bind(v),
            };
        }
        query
    }};
}

/// Страница записей схемы. `select` - список столбцов для `SELECT` (из кода сервиса, под структуру `R`).
/// Изоляция организаций остаётся за соединением: вызывать надо внутри транзакции тенанта (RLS).
pub async fn fetch_page<R>(
    conn: &mut PgConnection,
    schema: &GridSchema,
    select: &str,
    query: &GridQuery,
) -> Result<GridPage<R>, FetchError>
where
    R: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    let built = build(schema, query)?;
    let count_sql = format!("SELECT count(*) FROM {} {}", schema.source, built.where_sql);
    let total: i64 = bind_all!(sqlx::query_scalar(&count_sql), built.binds.iter().cloned())
        .fetch_one(&mut *conn)
        .await?;

    let rows_sql = format!(
        "SELECT {select} FROM {} {} {} LIMIT {} OFFSET {}",
        schema.source, built.where_sql, built.order_sql, built.limit, built.offset
    );
    let items: Vec<R> = bind_all!(sqlx::query_as(&rows_sql), built.binds.iter().cloned())
        .fetch_all(&mut *conn)
        .await?;
    Ok(GridPage {
        items,
        total: u64::try_from(total).unwrap_or(0),
        page: built.page,
        page_size: built.page_size,
    })
}
