//! Сборка SQL по запросу грида и схеме.
use chrono::{DateTime, NaiveDate, Utc};
use grid_contracts::{Filter, GridQuery, Logic, Operator, SortKey};
use platform_db::escape_like;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    ColumnType, GridColumn, GridError, GridSchema, MAX_CONDITIONS, MAX_DEPTH, MAX_SORT_KEYS,
    MAX_VALUE_LEN,
};

/// Значение параметра запроса.
#[derive(Debug, Clone, PartialEq)]
pub enum Bind {
    Text(String),
    Int(i64),
    Bool(bool),
    Uuid(Uuid),
    Timestamp(DateTime<Utc>),
    Date(NaiveDate),
}

/// Готовые части запроса. Параметры условий нумеруются с `$1`; `LIMIT` и `OFFSET` - числа,
/// проверенные сервером, поэтому вставлены в текст.
#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    /// `WHERE ...` либо пусто.
    pub where_sql: String,
    /// `ORDER BY ...` (всегда с ключом схемы).
    pub order_sql: String,
    pub binds: Vec<Bind>,
    pub limit: u32,
    pub offset: u64,
    pub page: u32,
    pub page_size: u32,
}

/// Проверяет запрос по схеме и лимитам и собирает SQL.
pub fn build(schema: &GridSchema, query: &GridQuery) -> Result<Built, GridError> {
    if query.page == 0 {
        return Err(GridError::new("page", "страницы нумеруются с 1"));
    }
    if query.page_size == 0 || query.page_size > schema.max_page_size {
        return Err(GridError::new(
            "page_size",
            format!("размер страницы от 1 до {}", schema.max_page_size),
        ));
    }
    let mut ctx = Ctx {
        schema,
        binds: Vec::new(),
        conditions: 0,
    };
    let where_sql = match &query.filter {
        Some(filter) => match ctx.group_or_leaf(filter, 1)? {
            Some(sql) => format!("WHERE {sql}"),
            None => String::new(),
        },
        None => String::new(),
    };
    Ok(Built {
        where_sql,
        order_sql: order_by(schema, &query.sort)?,
        binds: ctx.binds,
        limit: query.page_size,
        offset: query.offset(),
        page: query.page,
        page_size: query.page_size,
    })
}

fn order_by(schema: &GridSchema, sort: &[SortKey]) -> Result<String, GridError> {
    if sort.len() > MAX_SORT_KEYS {
        return Err(GridError::new(
            "sort",
            format!("не больше {MAX_SORT_KEYS} полей сортировки"),
        ));
    }
    let mut parts = Vec::new();
    let mut has_key = false;
    for key in sort {
        let column = schema
            .column(&key.field)
            .ok_or_else(|| GridError::new("sort", format!("неизвестное поле «{}»", key.field)))?;
        if !column.sortable {
            return Err(GridError::new(
                "sort",
                format!("по полю «{}» сортировать нельзя", key.field),
            ));
        }
        has_key |= column.name == schema.key;
        parts.push(format!(
            "({}) {}",
            column.sql,
            if key.desc { "DESC" } else { "ASC" }
        ));
    }
    if !has_key {
        let key = schema
            .column(schema.key)
            .expect("ключ схемы есть среди её столбцов");
        parts.push(format!("({}) ASC", key.sql));
    }
    Ok(format!("ORDER BY {}", parts.join(", ")))
}

struct Ctx<'a> {
    schema: &'a GridSchema,
    binds: Vec<Bind>,
    conditions: usize,
}

impl Ctx<'_> {
    /// `None` - условие пустое (группа без условий) и ничего не ограничивает.
    fn group_or_leaf(
        &mut self,
        filter: &Filter,
        depth: usize,
    ) -> Result<Option<String>, GridError> {
        match filter {
            Filter::Group { logic, filters } => {
                if depth > MAX_DEPTH {
                    return Err(GridError::new(
                        "filter",
                        format!("вложенность фильтра больше {MAX_DEPTH}"),
                    ));
                }
                let mut parts = Vec::new();
                for inner in filters {
                    if let Some(sql) = self.group_or_leaf(inner, depth + 1)? {
                        parts.push(sql);
                    }
                }
                Ok(match parts.len() {
                    0 => None,
                    1 => parts.pop(),
                    _ => {
                        let glue = match logic {
                            Logic::And => " AND ",
                            Logic::Or => " OR ",
                        };
                        Some(format!("({})", parts.join(glue)))
                    }
                })
            }
            Filter::Leaf {
                field,
                operator,
                value,
                ignore_case,
            } => {
                if depth > MAX_DEPTH {
                    return Err(GridError::new(
                        "filter",
                        format!("вложенность фильтра больше {MAX_DEPTH}"),
                    ));
                }
                self.conditions += 1;
                if self.conditions > MAX_CONDITIONS {
                    return Err(GridError::new(
                        "filter",
                        format!("условий больше {MAX_CONDITIONS}"),
                    ));
                }
                self.leaf(field, *operator, value.as_ref(), *ignore_case)
                    .map(Some)
            }
        }
    }

    fn leaf(
        &mut self,
        field: &str,
        op: Operator,
        value: Option<&Value>,
        ignore_case: bool,
    ) -> Result<String, GridError> {
        let column = *self
            .schema
            .column(field)
            .ok_or_else(|| GridError::new("filter", format!("неизвестное поле «{field}»")))?;
        if !column.filterable {
            return Err(GridError::new(
                "filter",
                format!("по полю «{field}» фильтровать нельзя"),
            ));
        }
        check_operator(&column, op)?;
        let col = format!("({})", column.sql);
        match op {
            Operator::IsNull => return Ok(format!("{col} IS NULL")),
            Operator::IsNotNull => return Ok(format!("{col} IS NOT NULL")),
            Operator::IsEmpty => return Ok(format!("({col} IS NULL OR {col} = '')")),
            Operator::IsNotEmpty => return Ok(format!("({col} IS NOT NULL AND {col} <> '')")),
            _ => {}
        }
        let value = value
            .ok_or_else(|| GridError::new("filter", format!("для «{field}» нужно значение")))?;
        match (column.ty, op) {
            (ColumnType::Text, Operator::Contains | Operator::StartsWith | Operator::EndsWith) => {
                let text = text_value(field, value)?;
                let escaped = escape_like(&text);
                let pattern = match op {
                    Operator::Contains => format!("%{escaped}%"),
                    Operator::StartsWith => format!("{escaped}%"),
                    _ => format!("%{escaped}"),
                };
                let n = self.push(Bind::Text(pattern));
                let like = if ignore_case { "ILIKE" } else { "LIKE" };
                Ok(format!("{col} {like} ${n}::text ESCAPE '\\'"))
            }
            (ColumnType::Text, Operator::Eq | Operator::Neq) if ignore_case => {
                let n = self.push(Bind::Text(text_value(field, value)?));
                let cmp = if op == Operator::Eq { "=" } else { "<>" };
                Ok(format!("lower({col}) {cmp} lower(${n}::text)"))
            }
            _ => {
                let (bind, cast) = typed_bind(&column, field, value)?;
                let n = self.push(bind);
                Ok(format!("{col} {} ${n}::{cast}", compare(op)))
            }
        }
    }

    fn push(&mut self, bind: Bind) -> usize {
        self.binds.push(bind);
        self.binds.len()
    }
}

fn compare(op: Operator) -> &'static str {
    match op {
        Operator::Eq => "=",
        Operator::Neq => "<>",
        Operator::Gt => ">",
        Operator::Gte => ">=",
        Operator::Lt => "<",
        Operator::Lte => "<=",
        _ => unreachable!("операторы сравнения"),
    }
}

fn check_operator(column: &GridColumn, op: Operator) -> Result<(), GridError> {
    use Operator::*;
    let ok = match column.ty {
        ColumnType::Text => true,
        ColumnType::Int | ColumnType::Timestamp | ColumnType::Date => {
            !matches!(op, Contains | StartsWith | EndsWith | IsEmpty | IsNotEmpty)
        }
        ColumnType::Bool | ColumnType::Uuid => {
            matches!(op, Eq | Neq | IsNull | IsNotNull)
        }
    };
    if ok {
        Ok(())
    } else {
        Err(GridError::new(
            "filter",
            format!("оператор не подходит полю «{}»", column.name),
        ))
    }
}

fn text_value(field: &str, value: &Value) -> Result<String, GridError> {
    let text = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => {
            return Err(GridError::new(
                "filter",
                format!("значение для «{field}» должно быть строкой"),
            ));
        }
    };
    if text.chars().count() > MAX_VALUE_LEN {
        return Err(GridError::new(
            "filter",
            format!("значение для «{field}» длиннее {MAX_VALUE_LEN} символов"),
        ));
    }
    Ok(text)
}

fn typed_bind(
    column: &GridColumn,
    field: &str,
    value: &Value,
) -> Result<(Bind, &'static str), GridError> {
    let bad = || {
        GridError::new(
            "filter",
            format!("значение для «{field}» не подходит типу поля"),
        )
    };
    Ok(match column.ty {
        ColumnType::Text => (Bind::Text(text_value(field, value)?), "text"),
        ColumnType::Int => {
            let n = match value {
                Value::Number(n) => n.as_i64(),
                Value::String(s) => s.trim().parse().ok(),
                _ => None,
            };
            (Bind::Int(n.ok_or_else(bad)?), "bigint")
        }
        ColumnType::Bool => {
            let b = match value {
                Value::Bool(b) => Some(*b),
                Value::String(s) => match s.as_str() {
                    "true" => Some(true),
                    "false" => Some(false),
                    _ => None,
                },
                _ => None,
            };
            (Bind::Bool(b.ok_or_else(bad)?), "boolean")
        }
        ColumnType::Uuid => {
            let text = text_value(field, value)?;
            (Bind::Uuid(text.parse().map_err(|_| bad())?), "uuid")
        }
        ColumnType::Timestamp => {
            let text = text_value(field, value)?;
            let parsed = DateTime::parse_from_rfc3339(&text)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|_| bad())?;
            (Bind::Timestamp(parsed), "timestamptz")
        }
        ColumnType::Date => {
            let text = text_value(field, value)?;
            (
                Bind::Date(NaiveDate::parse_from_str(&text, "%Y-%m-%d").map_err(|_| bad())?),
                "date",
            )
        }
    })
}

#[cfg(test)]
mod tests {
    use grid_contracts::{Filter, GridQuery, Operator, SortKey};
    use serde_json::json;

    use super::*;

    const COLUMNS: &[GridColumn] = &[
        GridColumn::new("id", "id", ColumnType::Uuid),
        GridColumn::new("email", "email", ColumnType::Text),
        GridColumn::new("created_at", "created_at", ColumnType::Timestamp),
        GridColumn::new("age", "age", ColumnType::Int),
        GridColumn::new("active", "active", ColumnType::Bool),
        GridColumn::new("secret", "secret", ColumnType::Text)
            .no_filter()
            .no_sort(),
    ];
    const SCHEMA: GridSchema = GridSchema {
        source: "people",
        key: "id",
        columns: COLUMNS,
        max_page_size: 100,
    };

    fn built(filter: Filter) -> Built {
        build(
            &SCHEMA,
            &GridQuery {
                filter: Some(filter),
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn rejected(query: GridQuery) -> GridError {
        build(&SCHEMA, &query).unwrap_err()
    }

    fn with_filter(filter: Filter) -> GridQuery {
        GridQuery {
            filter: Some(filter),
            ..Default::default()
        }
    }

    #[test]
    fn an_empty_query_orders_by_the_key() {
        let b = build(&SCHEMA, &GridQuery::default()).unwrap();
        assert_eq!(b.where_sql, "");
        assert_eq!(b.order_sql, "ORDER BY (id) ASC");
        assert_eq!((b.limit, b.offset, b.page), (20, 0, 1));
        assert!(b.binds.is_empty());
    }

    #[test]
    fn sorting_is_case_insensitive_and_keeps_a_stable_tail() {
        let b = build(
            &SCHEMA,
            &GridQuery {
                sort: vec![SortKey::desc("CreatedAt"), SortKey::asc("email")],
                page: 3,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            b.order_sql,
            "ORDER BY (created_at) DESC, (email) ASC, (id) ASC"
        );
        assert_eq!(b.offset, 40);
    }

    #[test]
    fn sorting_by_the_key_does_not_repeat_it() {
        let b = build(
            &SCHEMA,
            &GridQuery {
                sort: vec![SortKey::desc("id")],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(b.order_sql, "ORDER BY (id) DESC");
    }

    #[test]
    fn string_operators_escape_wildcards_and_bind_values() {
        let b = built(Filter::leaf("email", Operator::Contains, "100%_x'--"));
        assert_eq!(b.where_sql, "WHERE (email) ILIKE $1::text ESCAPE '\\'");
        assert_eq!(b.binds, vec![Bind::Text("%100\\%\\_x'--%".into())]);
        let b = built(Filter::leaf("email", Operator::StartsWith, "a"));
        assert_eq!(b.binds, vec![Bind::Text("a%".into())]);
        let b = built(Filter::leaf("email", Operator::EndsWith, "a"));
        assert_eq!(b.binds, vec![Bind::Text("%a".into())]);
    }

    #[test]
    fn equality_ignores_case_unless_asked() {
        let b = built(Filter::leaf("email", Operator::Eq, "A@b.io"));
        assert_eq!(b.where_sql, "WHERE lower((email)) = lower($1::text)");
        let b = built(Filter::Leaf {
            field: "email".into(),
            operator: Operator::Eq,
            value: Some(json!("A@b.io")),
            ignore_case: false,
        });
        assert_eq!(b.where_sql, "WHERE (email) = $1::text");
    }

    #[test]
    fn groups_combine_with_their_logic_and_number_parameters() {
        let b = built(Filter::and(vec![
            Filter::leaf("age", Operator::Gte, 18),
            Filter::or(vec![
                Filter::leaf("email", Operator::Contains, "ann"),
                Filter::nullary("email", Operator::IsEmpty),
            ]),
            Filter::leaf("active", Operator::Eq, true),
        ]));
        assert_eq!(
            b.where_sql,
            "WHERE ((age) >= $1::bigint AND ((email) ILIKE $2::text ESCAPE '\\' OR ((email) IS NULL OR (email) = '')) AND (active) = $3::boolean)"
        );
        assert_eq!(b.binds.len(), 3);
    }

    #[test]
    fn empty_groups_do_not_restrict() {
        let b = built(Filter::and(vec![Filter::or(vec![])]));
        assert_eq!(b.where_sql, "");
    }

    #[test]
    fn values_are_converted_to_the_column_type() {
        let b = built(Filter::leaf("age", Operator::Lt, "7"));
        assert_eq!(b.binds, vec![Bind::Int(7)]);
        let id = Uuid::new_v4();
        let b = built(Filter::leaf("id", Operator::Eq, id.to_string()));
        assert_eq!(b.binds, vec![Bind::Uuid(id)]);
        let b = built(Filter::leaf(
            "created_at",
            Operator::Gt,
            "2026-01-02T03:04:05Z",
        ));
        assert!(matches!(b.binds[0], Bind::Timestamp(_)));
    }

    #[test]
    fn unknown_fields_and_forbidden_operations_are_rejected_with_the_parameter() {
        assert_eq!(
            rejected(with_filter(Filter::leaf("nope", Operator::Eq, 1))).param,
            "filter"
        );
        assert_eq!(
            rejected(with_filter(Filter::leaf("secret", Operator::Eq, "x"))).param,
            "filter"
        );
        let sort = |f: &str| GridQuery {
            sort: vec![SortKey::asc(f)],
            ..Default::default()
        };
        assert_eq!(rejected(sort("nope")).param, "sort");
        assert_eq!(rejected(sort("secret")).param, "sort");
        assert_eq!(rejected(sort("password_hash")).param, "sort");
    }

    #[test]
    fn operators_must_fit_the_type_and_values_must_parse() {
        for filter in [
            Filter::leaf("age", Operator::Contains, "1"),
            Filter::nullary("age", Operator::IsEmpty),
            Filter::leaf("active", Operator::Gt, true),
            Filter::leaf("age", Operator::Eq, "abc"),
            Filter::leaf("id", Operator::Eq, "not-a-uuid"),
            Filter::leaf("created_at", Operator::Eq, "yesterday"),
            Filter::leaf("active", Operator::Eq, "maybe"),
            Filter::leaf("email", Operator::Eq, json!({"a": 1})),
        ] {
            assert_eq!(rejected(with_filter(filter)).param, "filter");
        }
        let missing = Filter::Leaf {
            field: "email".into(),
            operator: Operator::Eq,
            value: None,
            ignore_case: true,
        };
        assert_eq!(rejected(with_filter(missing)).param, "filter");
    }

    #[test]
    fn limits_are_enforced() {
        assert_eq!(
            rejected(GridQuery {
                page_size: 101,
                ..Default::default()
            })
            .param,
            "page_size"
        );
        assert_eq!(
            rejected(GridQuery {
                page_size: 0,
                ..Default::default()
            })
            .param,
            "page_size"
        );
        assert_eq!(
            rejected(GridQuery {
                page: 0,
                ..Default::default()
            })
            .param,
            "page"
        );
        let long = "x".repeat(MAX_VALUE_LEN + 1);
        assert_eq!(
            rejected(with_filter(Filter::leaf("email", Operator::Eq, long))).param,
            "filter"
        );
        let many = Filter::and(vec![
            Filter::leaf("age", Operator::Gt, 1);
            MAX_CONDITIONS + 1
        ]);
        assert_eq!(rejected(with_filter(many)).param, "filter");
        let mut deep = Filter::leaf("age", Operator::Gt, 1);
        for _ in 0..MAX_DEPTH {
            deep = Filter::and(vec![deep]);
        }
        assert_eq!(rejected(with_filter(deep)).param, "filter");
        let sorts = GridQuery {
            sort: vec![SortKey::asc("email"); MAX_SORT_KEYS + 1],
            ..Default::default()
        };
        assert_eq!(rejected(sorts).param, "sort");
    }

    #[test]
    fn a_hostile_value_never_reaches_the_sql_text() {
        let evil = "'; DROP TABLE users; --";
        let b = built(Filter::leaf("email", Operator::Eq, evil));
        assert!(!b.where_sql.contains("DROP"));
        assert!(!b.order_sql.contains("DROP"));
        assert_eq!(b.binds, vec![Bind::Text(evil.into())]);
    }
}
