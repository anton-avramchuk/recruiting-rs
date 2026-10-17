//! Адрес запроса списка: базовый путь и параметры грида.
use grid_contracts::GridQuery;

use crate::encode_query_value;

/// Путь списка с параметрами грида: значения по умолчанию опускаются, значения кодируются.
/// Если в `base` уже есть параметры, добавляются через `&`.
pub fn grid_path(base: &str, query: &GridQuery) -> String {
    let params = query
        .to_pairs()
        .into_iter()
        .map(|(key, value)| format!("{key}={}", encode_query_value(&value)))
        .collect::<Vec<_>>()
        .join("&");
    if params.is_empty() {
        base.to_string()
    } else if base.contains('?') {
        format!("{base}&{params}")
    } else {
        format!("{base}?{params}")
    }
}

#[cfg(test)]
mod tests {
    use grid_contracts::{Filter, Operator, SortKey};

    use super::*;

    #[test]
    fn a_default_query_leaves_the_path_alone() {
        assert_eq!(
            grid_path("/api/persons", &GridQuery::default()),
            "/api/persons"
        );
    }

    #[test]
    fn page_size_sort_and_filter_are_encoded() {
        let query = GridQuery {
            page: 2,
            page_size: 50,
            sort: vec![SortKey::desc("full_name")],
            filter: Some(Filter::leaf("source", Operator::Eq, "hh.ru")),
        };
        let path = grid_path("/api/persons", &query);
        assert!(path.starts_with("/api/persons?page=2&page_size=50&sort=full_name%3Adesc&filter="));
        assert!(path.contains("%22field%22%3A%22source%22"));
        assert!(!path.contains(' ') && !path.contains('{'));
    }

    #[test]
    fn existing_parameters_are_continued_with_an_ampersand() {
        let query = GridQuery {
            page: 3,
            ..GridQuery::default()
        };
        assert_eq!(
            grid_path("/api/clients?x=1", &query),
            "/api/clients?x=1&page=3"
        );
    }
}
