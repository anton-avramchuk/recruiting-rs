//! Состояние грида в адресной строке и его перевод в запрос к серверу. Без Leptos: проверяется юнит-тестами.
use std::collections::BTreeMap;

use grid_contracts::{DEFAULT_PAGE_SIZE, Filter, GridQuery, Operator, SortKey, parse_sort};

use crate::encode_query_value;

/// Допустимые размеры страницы в выборе.
pub const PAGE_SIZES: [u32; 3] = [20, 50, 100];

/// Как фильтр столбца превращается в условие.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterSpec {
    pub field: &'static str,
    pub op: Operator,
}

/// Представление таблицы, которое хранится в адресе: `page`, `page_size`, `sort`, `q`, `f.<поле>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridUrl {
    pub page: u32,
    pub page_size: u32,
    /// Сортировка по одному полю (клик по заголовку).
    pub sort: Option<SortKey>,
    pub q: String,
    /// Фильтры столбцов: поле -> непустое значение.
    pub filters: BTreeMap<String, String>,
}

impl Default for GridUrl {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: DEFAULT_PAGE_SIZE,
            sort: None,
            q: String::new(),
            filters: BTreeMap::new(),
        }
    }
}

impl GridUrl {
    /// Читает состояние через `get(имя параметра)`; мусор молча заменяется значениями по умолчанию.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>, filter_fields: &[&str]) -> Self {
        let page = get("page")
            .and_then(|v| v.trim().parse().ok())
            .filter(|p| *p >= 1)
            .unwrap_or(1);
        let page_size = get("page_size")
            .and_then(|v| v.trim().parse().ok())
            .filter(|s| PAGE_SIZES.contains(s))
            .unwrap_or(DEFAULT_PAGE_SIZE);
        let sort = get("sort")
            .and_then(|v| parse_sort(&v).ok())
            .and_then(|keys| keys.into_iter().next());
        let q = get("q").unwrap_or_default().trim().to_string();
        let filters = filter_fields
            .iter()
            .filter_map(|field| {
                get(&format!("f.{field}"))
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .map(|v| ((*field).to_string(), v))
            })
            .collect();
        Self {
            page,
            page_size,
            sort,
            q,
            filters,
        }
    }

    /// Строка запроса адреса (без `?`); значения по умолчанию опускаются.
    pub fn to_query_string(&self) -> String {
        let mut parts = Vec::new();
        if self.page != 1 {
            parts.push(format!("page={}", self.page));
        }
        if self.page_size != DEFAULT_PAGE_SIZE {
            parts.push(format!("page_size={}", self.page_size));
        }
        if let Some(sort) = &self.sort {
            let dir = if sort.desc { "desc" } else { "asc" };
            parts.push(format!(
                "sort={}",
                encode_query_value(&format!("{}:{dir}", sort.field))
            ));
        }
        if !self.q.is_empty() {
            parts.push(format!("q={}", encode_query_value(&self.q)));
        }
        for (field, value) in &self.filters {
            parts.push(format!("f.{field}={}", encode_query_value(value)));
        }
        parts.join("&")
    }

    /// Есть ли что сбрасывать (поиск, фильтры, сортировка).
    pub fn is_filtered(&self) -> bool {
        !self.q.is_empty() || !self.filters.is_empty() || self.sort.is_some()
    }

    /// Только фильтры и поиск (без сортировки): для пустого состояния «ничего не найдено».
    pub fn has_conditions(&self) -> bool {
        !self.q.is_empty() || !self.filters.is_empty()
    }

    pub fn with_page(mut self, page: u32) -> Self {
        self.page = page.max(1);
        self
    }

    pub fn with_page_size(mut self, size: u32) -> Self {
        self.page_size = if PAGE_SIZES.contains(&size) {
            size
        } else {
            DEFAULT_PAGE_SIZE
        };
        self.page = 1;
        self
    }

    pub fn with_q(mut self, q: &str) -> Self {
        self.q = q.trim().to_string();
        self.page = 1;
        self
    }

    pub fn with_filter(mut self, field: &str, value: &str) -> Self {
        let value = value.trim();
        if value.is_empty() {
            self.filters.remove(field);
        } else {
            self.filters.insert(field.to_string(), value.to_string());
        }
        self.page = 1;
        self
    }

    /// Клик по заголовку: по возрастанию, затем по убыванию, затем без сортировки.
    pub fn toggle_sort(mut self, field: &str) -> Self {
        self.sort = match self.sort.take() {
            Some(s) if s.field == field && !s.desc => Some(SortKey::desc(field)),
            Some(s) if s.field == field => None,
            _ => Some(SortKey::asc(field)),
        };
        self.page = 1;
        self
    }

    /// Сброс поиска, фильтров и сортировки; размер страницы сохраняется.
    pub fn reset(self) -> Self {
        Self {
            page_size: self.page_size,
            ..Self::default()
        }
    }

    /// Запрос к серверу: фильтры столбцов складываются по «И», поиск - «ИЛИ» по полям поиска.
    pub fn to_grid_query(&self, specs: &[FilterSpec], search_fields: &[&str]) -> GridQuery {
        let mut parts: Vec<Filter> = Vec::new();
        for (field, value) in &self.filters {
            if let Some(spec) = specs.iter().find(|s| s.field == field) {
                parts.push(Filter::leaf(spec.field, spec.op, value.as_str()));
            }
        }
        if !self.q.is_empty() && !search_fields.is_empty() {
            let hits = search_fields
                .iter()
                .map(|f| Filter::leaf(*f, Operator::Contains, self.q.as_str()))
                .collect();
            parts.push(Filter::or(hits));
        }
        let filter = match parts.len() {
            0 => None,
            1 => parts.pop(),
            _ => Some(Filter::and(parts)),
        };
        GridQuery {
            page: self.page,
            page_size: self.page_size,
            sort: self.sort.clone().into_iter().collect(),
            filter,
        }
    }
}

/// Подпись пейджера: «1–20 из 135» либо «Ничего нет».
pub fn page_info(page: u32, page_size: u32, shown: usize, total: u64) -> String {
    if total == 0 || shown == 0 {
        return "Ничего нет".into();
    }
    let from = u64::from(page.saturating_sub(1)) * u64::from(page_size) + 1;
    let to = from + shown as u64 - 1;
    format!("{from}–{to} из {total}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(pairs: &[(&str, &str)]) -> GridUrl {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        GridUrl::from_lookup(
            |key| owned.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()),
            &["status", "role"],
        )
    }

    #[test]
    fn an_empty_address_gives_defaults() {
        let u = url(&[]);
        assert_eq!(u, GridUrl::default());
        assert_eq!(u.to_query_string(), "");
        assert!(!u.is_filtered());
    }

    #[test]
    fn the_state_survives_a_round_trip_through_the_address() {
        let u = GridUrl::default()
            .with_page_size(50)
            .with_filter("status", "blocked")
            .with_filter("role", "viewer")
            .with_q("иван & co")
            .toggle_sort("email")
            .toggle_sort("email")
            .with_page(3);
        assert_eq!(u.sort, Some(SortKey::desc("email")));
        let qs = u.to_query_string();
        let decoded: Vec<(String, String)> = qs
            .split('&')
            .map(|p| {
                let (k, v) = p.split_once('=').unwrap();
                (k.to_string(), percent_decode(v))
            })
            .collect();
        let back = GridUrl::from_lookup(
            |key| {
                decoded
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.clone())
            },
            &["status", "role"],
        );
        assert_eq!(back, u);
    }

    fn percent_decode(v: &str) -> String {
        let bytes = v.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                out.push(u8::from_str_radix(&v[i + 1..i + 3], 16).unwrap());
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn garbage_in_the_address_falls_back_to_defaults() {
        let u = url(&[
            ("page", "abc"),
            ("page_size", "7"),
            ("sort", "email:sideways"),
            ("f.status", "  "),
            ("f.unknown", "x"),
        ]);
        assert_eq!(u, GridUrl::default());
        assert_eq!(url(&[("page", "0")]).page, 1);
        assert_eq!(url(&[("page_size", "100")]).page_size, 100);
    }

    #[test]
    fn changing_a_filter_or_search_returns_to_the_first_page() {
        let base = GridUrl::default().with_page(4);
        assert_eq!(base.clone().with_filter("status", "active").page, 1);
        assert_eq!(base.clone().with_q("a").page, 1);
        assert_eq!(base.clone().with_page_size(50).page, 1);
        assert_eq!(base.toggle_sort("email").page, 1);
    }

    #[test]
    fn sorting_cycles_ascending_descending_none() {
        let u = GridUrl::default().toggle_sort("email");
        assert_eq!(u.sort, Some(SortKey::asc("email")));
        let u = u.toggle_sort("email");
        assert_eq!(u.sort, Some(SortKey::desc("email")));
        let u = u.toggle_sort("email");
        assert_eq!(u.sort, None);
        let u = GridUrl::default()
            .toggle_sort("email")
            .toggle_sort("status");
        assert_eq!(u.sort, Some(SortKey::asc("status")));
    }

    #[test]
    fn an_empty_value_removes_the_filter_and_reset_keeps_the_page_size() {
        let u = GridUrl::default()
            .with_page_size(100)
            .with_filter("status", "x")
            .with_filter("status", " ");
        assert!(u.filters.is_empty());
        let u = u.with_q("a").toggle_sort("email").reset();
        assert_eq!(
            (u.page_size, u.q.as_str(), u.sort.is_none()),
            (100, "", true)
        );
    }

    const SPECS: [FilterSpec; 2] = [
        FilterSpec {
            field: "status",
            op: Operator::Eq,
        },
        FilterSpec {
            field: "role",
            op: Operator::Contains,
        },
    ];

    #[test]
    fn column_filters_and_search_become_one_server_filter() {
        let none = GridUrl::default().to_grid_query(&SPECS, &["email"]);
        assert_eq!(none, GridQuery::default());

        let one = GridUrl::default()
            .with_filter("status", "blocked")
            .to_grid_query(&SPECS, &["email", "display_name"]);
        assert_eq!(
            one.filter,
            Some(Filter::leaf("status", Operator::Eq, "blocked"))
        );

        let both = GridUrl::default()
            .with_filter("role", "viewer")
            .with_q("ann")
            .toggle_sort("email")
            .with_page(2)
            .to_grid_query(&SPECS, &["email", "display_name"]);
        assert_eq!((both.page, both.sort.len()), (2, 1));
        assert_eq!(
            both.filter,
            Some(Filter::and(vec![
                Filter::leaf("role", Operator::Contains, "viewer"),
                Filter::or(vec![
                    Filter::leaf("email", Operator::Contains, "ann"),
                    Filter::leaf("display_name", Operator::Contains, "ann"),
                ]),
            ]))
        );
    }

    #[test]
    fn search_without_search_fields_and_unknown_filters_are_ignored() {
        let mut u = GridUrl::default().with_q("x");
        u.filters.insert("ghost".into(), "1".into());
        assert_eq!(u.to_grid_query(&SPECS, &[]).filter, None);
    }

    #[test]
    fn the_pager_caption() {
        assert_eq!(page_info(1, 20, 20, 135), "1–20 из 135");
        assert_eq!(page_info(7, 20, 15, 135), "121–135 из 135");
        assert_eq!(page_info(1, 20, 0, 0), "Ничего нет");
    }
}
