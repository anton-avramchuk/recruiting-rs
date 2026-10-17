//! Правки фильтра запроса грида на стороне приложения: вынуть условие, добавить условие.
use grid_contracts::{Filter, GridQuery, Operator};

/// Находит условие `поле = значение` на любом уровне фильтра, убирает его (с опустевшими группами)
/// и возвращает значение. Если таких условий несколько, убирается первое.
pub fn take_eq(query: &mut GridQuery, field: &str) -> Option<String> {
    let filter = query.filter.take()?;
    let mut found = None;
    query.filter = strip(filter, field, &mut found);
    found
}

fn strip(filter: Filter, field: &str, found: &mut Option<String>) -> Option<Filter> {
    match filter {
        Filter::Leaf {
            field: ref name,
            operator: Operator::Eq,
            ref value,
            ..
        } if found.is_none() && name == field => {
            *found = value.as_ref().and_then(|v| v.as_str()).map(str::to_string);
            // условие с чужим видом значения не удаляем
            found.is_none().then_some(filter)
        }
        Filter::Leaf { .. } => Some(filter),
        Filter::Group { logic, filters } => {
            let kept: Vec<Filter> = filters
                .into_iter()
                .filter_map(|f| strip(f, field, found))
                .collect();
            (!kept.is_empty()).then_some(Filter::Group {
                logic,
                filters: kept,
            })
        }
    }
}

/// Добавляет условие к фильтру через И.
pub fn and_condition(query: &mut GridQuery, condition: Filter) {
    query.filter = Some(match query.filter.take() {
        Some(existing) => Filter::and(vec![existing, condition]),
        None => condition,
    });
}

#[cfg(test)]
mod tests {
    use grid_contracts::Logic;

    use super::*;

    fn query(filter: Filter) -> GridQuery {
        GridQuery {
            filter: Some(filter),
            ..GridQuery::default()
        }
    }

    #[test]
    fn a_top_level_condition_is_taken_and_the_filter_becomes_empty() {
        let mut q = query(Filter::leaf("client", Operator::Eq, "c1"));
        assert_eq!(take_eq(&mut q, "client").as_deref(), Some("c1"));
        assert!(q.filter.is_none());
    }

    #[test]
    fn a_nested_condition_is_taken_and_the_rest_stays() {
        let mut q = query(Filter::and(vec![
            Filter::leaf("client", Operator::Eq, "c1"),
            Filter::or(vec![
                Filter::leaf("full_name", Operator::Contains, "a"),
                Filter::leaf("email", Operator::Contains, "a"),
            ]),
        ]));
        assert_eq!(take_eq(&mut q, "client").as_deref(), Some("c1"));
        let Some(Filter::Group { logic, filters }) = &q.filter else {
            panic!("a group is expected");
        };
        assert_eq!((*logic, filters.len()), (Logic::And, 1));
    }

    #[test]
    fn other_fields_and_other_operators_are_left_alone() {
        let mut q = query(Filter::and(vec![
            Filter::leaf("source", Operator::Eq, "hh"),
            Filter::leaf("client", Operator::Contains, "c1"),
        ]));
        assert_eq!(take_eq(&mut q, "client"), None);
        assert!(q.filter.is_some());
        assert_eq!(take_eq(&mut GridQuery::default(), "client"), None);
    }

    #[test]
    fn a_condition_is_added_with_and() {
        let mut q = GridQuery::default();
        and_condition(
            &mut q,
            Filter::leaf("id", Operator::In, serde_json::json!(["x"])),
        );
        assert!(matches!(q.filter, Some(Filter::Leaf { .. })));
        and_condition(&mut q, Filter::leaf("source", Operator::Eq, "hh"));
        assert!(matches!(
            q.filter,
            Some(Filter::Group {
                logic: Logic::And,
                ..
            })
        ));
    }
}
