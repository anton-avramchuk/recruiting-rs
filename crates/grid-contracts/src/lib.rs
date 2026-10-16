//! Общий контракт грида: запрос страницы с сортировкой и рекурсивным фильтром и ответ-страница.
//! Не зависит ни от сервера, ни от браузера: используется сервисами, BFF и интерфейсом.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Размер страницы по умолчанию.
pub const DEFAULT_PAGE_SIZE: u32 = 20;

/// Ошибка разбора параметров запроса: какой параметр и что с ним не так.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryError {
    pub param: &'static str,
    pub message: String,
}

impl QueryError {
    fn new(param: &'static str, message: impl Into<String>) -> Self {
        Self {
            param,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.param, self.message)
    }
}

impl std::error::Error for QueryError {}

/// Ключ сортировки: поле и направление.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SortKey {
    pub field: String,
    #[serde(default)]
    pub desc: bool,
}

impl SortKey {
    pub fn asc(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            desc: false,
        }
    }

    pub fn desc(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            desc: true,
        }
    }
}

/// Как объединять вложенные условия.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum Logic {
    And,
    Or,
}

/// Оператор условия.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum Operator {
    Eq,
    Neq,
    Contains,
    StartsWith,
    EndsWith,
    Gt,
    Gte,
    Lt,
    Lte,
    IsNull,
    IsNotNull,
    IsEmpty,
    IsNotEmpty,
}

impl Operator {
    /// Нужен ли оператору operand (`value`).
    pub fn takes_value(self) -> bool {
        !matches!(
            self,
            Operator::IsNull | Operator::IsNotNull | Operator::IsEmpty | Operator::IsNotEmpty
        )
    }
}

fn yes() -> bool {
    true
}

/// Рекурсивный фильтр: группа условий либо условие на поле.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum Filter {
    Group {
        logic: Logic,
        #[cfg_attr(feature = "openapi", schema(no_recursion))]
        filters: Vec<Filter>,
    },
    Leaf {
        field: String,
        operator: Operator,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<Value>,
        /// Игнорировать регистр при сравнении строк (по умолчанию да).
        #[serde(default = "yes", rename = "ignore_case")]
        ignore_case: bool,
    },
}

impl Filter {
    pub fn leaf(field: impl Into<String>, operator: Operator, value: impl Into<Value>) -> Self {
        Filter::Leaf {
            field: field.into(),
            operator,
            value: Some(value.into()),
            ignore_case: true,
        }
    }

    pub fn nullary(field: impl Into<String>, operator: Operator) -> Self {
        Filter::Leaf {
            field: field.into(),
            operator,
            value: None,
            ignore_case: true,
        }
    }

    pub fn and(filters: Vec<Filter>) -> Self {
        Filter::Group {
            logic: Logic::And,
            filters,
        }
    }

    pub fn or(filters: Vec<Filter>) -> Self {
        Filter::Group {
            logic: Logic::Or,
            filters,
        }
    }
}

/// Запрос грида: страница, размер страницы, сортировка и фильтр.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GridQuery {
    /// Номер страницы, с 1.
    pub page: u32,
    pub page_size: u32,
    pub sort: Vec<SortKey>,
    pub filter: Option<Filter>,
}

impl Default for GridQuery {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: DEFAULT_PAGE_SIZE,
            sort: Vec::new(),
            filter: None,
        }
    }
}

/// Нормализация имени поля: без регистра и `_` (`createdAt` = `created_at`).
pub fn normalize_field(name: &str) -> String {
    name.chars()
        .filter(|c| *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

impl GridQuery {
    /// Самый длинный допустимый параметр `filter`, байт.
    pub const MAX_FILTER_LEN: usize = 4096;

    /// Разбирает пары параметров `page`, `page_size`, `sort`, `filter`; остальные игнорируются.
    pub fn from_pairs<'a>(
        pairs: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Self, QueryError> {
        let mut query = Self::default();
        for (key, value) in pairs {
            match key {
                "page" => query.page = parse_number("page", value)?,
                "page_size" => query.page_size = parse_number("page_size", value)?,
                "sort" => query.sort = parse_sort(value)?,
                "filter" => query.filter = parse_filter(value)?,
                _ => {}
            }
        }
        Ok(query)
    }

    /// Параметры запроса для адреса; значения по умолчанию опускаются. Значения не кодируются.
    pub fn to_pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        if self.page != 1 {
            pairs.push(("page", self.page.to_string()));
        }
        if self.page_size != DEFAULT_PAGE_SIZE {
            pairs.push(("page_size", self.page_size.to_string()));
        }
        if !self.sort.is_empty() {
            pairs.push(("sort", format_sort(&self.sort)));
        }
        if let Some(filter) = &self.filter {
            pairs.push((
                "filter",
                serde_json::to_string(filter).expect("filter serializes"),
            ));
        }
        pairs
    }

    /// Смещение первой записи страницы.
    pub fn offset(&self) -> u64 {
        u64::from(self.page.saturating_sub(1)) * u64::from(self.page_size)
    }
}

fn parse_number(param: &'static str, value: &str) -> Result<u32, QueryError> {
    value
        .trim()
        .parse()
        .map_err(|_| QueryError::new(param, "ожидается целое число"))
}

/// `email:asc,created_at:desc`; направление по умолчанию - по возрастанию.
pub fn parse_sort(value: &str) -> Result<Vec<SortKey>, QueryError> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (field, dir) = part.split_once(':').unwrap_or((part, "asc"));
            let desc = match dir.trim().to_lowercase().as_str() {
                "asc" => false,
                "desc" => true,
                other => {
                    return Err(QueryError::new(
                        "sort",
                        format!("направление «{other}» не поддерживается"),
                    ));
                }
            };
            let field = field.trim();
            if field.is_empty() {
                return Err(QueryError::new("sort", "пустое имя поля"));
            }
            Ok(SortKey {
                field: field.to_string(),
                desc,
            })
        })
        .collect()
}

pub fn format_sort(keys: &[SortKey]) -> String {
    keys.iter()
        .map(|k| format!("{}:{}", k.field, if k.desc { "desc" } else { "asc" }))
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_filter(value: &str) -> Result<Option<Filter>, QueryError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > GridQuery::MAX_FILTER_LEN {
        return Err(QueryError::new("filter", "фильтр слишком длинный"));
    }
    serde_json::from_str(value)
        .map(Some)
        .map_err(|_| QueryError::new("filter", "фильтр не разобран"))
}

/// Страница результата.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GridPage<T> {
    pub items: Vec<T>,
    /// Записей всего (с учётом фильтра).
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

impl<T> GridPage<T> {
    /// Число страниц (не меньше одной).
    pub fn pages(&self) -> u32 {
        let size = u64::from(self.page_size.max(1));
        u32::try_from(self.total.div_ceil(size).max(1)).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn defaults_apply_when_nothing_is_given() {
        let q = GridQuery::from_pairs([]).unwrap();
        assert_eq!(q, GridQuery::default());
        assert_eq!((q.page, q.page_size), (1, 20));
    }

    #[test]
    fn a_query_survives_a_round_trip_through_pairs() {
        let q = GridQuery {
            page: 3,
            page_size: 50,
            sort: vec![SortKey::asc("status"), SortKey::desc("created_at")],
            filter: Some(Filter::and(vec![
                Filter::leaf("status", Operator::Eq, "blocked"),
                Filter::or(vec![
                    Filter::leaf("email", Operator::Contains, "ann"),
                    Filter::nullary("display_name", Operator::IsEmpty),
                ]),
            ])),
        };
        let pairs = q.to_pairs();
        let back = GridQuery::from_pairs(pairs.iter().map(|(k, v)| (*k, v.as_str()))).unwrap();
        assert_eq!(back, q);
    }

    #[test]
    fn default_values_are_not_written_to_the_address() {
        assert!(GridQuery::default().to_pairs().is_empty());
    }

    #[test]
    fn sort_accepts_a_missing_direction_and_spaces() {
        assert_eq!(
            parse_sort(" email , created_at:DESC ").unwrap(),
            vec![SortKey::asc("email"), SortKey::desc("created_at")]
        );
        assert!(parse_sort("").unwrap().is_empty());
    }

    #[test]
    fn bad_input_names_the_parameter() {
        let err = |pairs: &[(&str, &str)]| {
            GridQuery::from_pairs(pairs.iter().copied())
                .unwrap_err()
                .param
        };
        assert_eq!(err(&[("page", "abc")]), "page");
        assert_eq!(err(&[("page_size", "-1")]), "page_size");
        assert_eq!(err(&[("sort", "email:sideways")]), "sort");
        assert_eq!(err(&[("sort", ":asc")]), "sort");
        assert_eq!(err(&[("filter", "{not json")]), "filter");
        let long = format!("\"{}\"", "x".repeat(GridQuery::MAX_FILTER_LEN));
        assert_eq!(err(&[("filter", long.as_str())]), "filter");
    }

    #[test]
    fn the_filter_json_has_a_stable_shape() {
        let f: Filter = serde_json::from_value(json!({
            "logic": "or",
            "filters": [
                {"field": "email", "operator": "startswith", "value": "a"},
                {"field": "x", "operator": "isnull"}
            ]
        }))
        .unwrap();
        let Filter::Group { logic, filters } = f else {
            panic!("group expected");
        };
        assert_eq!((logic, filters.len()), (Logic::Or, 2));
        assert!(matches!(
            &filters[0],
            Filter::Leaf {
                operator: Operator::StartsWith,
                ignore_case: true,
                ..
            }
        ));
    }

    #[test]
    fn an_unknown_operator_is_rejected() {
        let r: Result<Filter, _> =
            serde_json::from_value(json!({"field": "a", "operator": "like", "value": 1}));
        assert!(r.is_err());
    }

    #[test]
    fn field_names_match_across_styles() {
        assert_eq!(normalize_field("createdAt"), normalize_field("created_at"));
        assert_ne!(normalize_field("created"), normalize_field("created_at"));
    }

    #[test]
    fn page_math() {
        let page = GridPage::<u8> {
            items: vec![],
            total: 41,
            page: 1,
            page_size: 20,
        };
        assert_eq!(page.pages(), 3);
        let empty = GridPage::<u8> { total: 0, ..page };
        assert_eq!(empty.pages(), 1);
        assert_eq!(
            GridQuery {
                page: 3,
                page_size: 20,
                ..Default::default()
            }
            .offset(),
            40
        );
    }
}
