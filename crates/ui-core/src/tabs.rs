//! Какая вкладка текущая по адресу страницы.

/// Индекс текущей вкладки: побеждает самый длинный адрес вкладки, который совпадает с путём целиком
/// или является его началом по границе сегмента (`/security` подходит к `/security/users/1`, но не к `/securityx`).
pub fn active_tab(path: &str, hrefs: &[&str]) -> Option<usize> {
    let path = path.split(['?', '#']).next().unwrap_or(path);
    hrefs
        .iter()
        .enumerate()
        .filter(|(_, href)| matches_prefix(path, href))
        .max_by_key(|(_, href)| href.len())
        .map(|(i, _)| i)
}

fn matches_prefix(path: &str, href: &str) -> bool {
    let href = href.trim_end_matches('/');
    if href.is_empty() {
        return path == "/" || path.is_empty();
    }
    path == href
        || path
            .strip_prefix(href)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABS: [&str; 2] = ["/security", "/security/roles"];

    #[test]
    fn the_exact_address_is_current() {
        assert_eq!(active_tab("/security", &TABS), Some(0));
        assert_eq!(active_tab("/security/roles", &TABS), Some(1));
    }

    #[test]
    fn nested_pages_belong_to_the_closest_tab() {
        assert_eq!(active_tab("/security/users/42", &TABS), Some(0));
        assert_eq!(active_tab("/security/roles/new", &TABS), Some(1));
        assert_eq!(active_tab("/security/roles/7/edit", &TABS), Some(1));
    }

    #[test]
    fn a_lookalike_prefix_is_not_a_match() {
        assert_eq!(active_tab("/securityx", &TABS), None);
        assert_eq!(active_tab("/secur", &TABS), None);
        assert_eq!(active_tab("/other", &TABS), None);
    }

    #[test]
    fn the_query_and_a_trailing_slash_are_ignored() {
        assert_eq!(active_tab("/security?page=2", &TABS), Some(0));
        assert_eq!(active_tab("/security/roles/", &TABS), Some(1));
        assert_eq!(active_tab("/security/", &["/security/"]), Some(0));
    }

    #[test]
    fn the_root_tab_matches_only_the_root() {
        assert_eq!(active_tab("/", &["/", "/clients"]), Some(0));
        assert_eq!(active_tab("/clients", &["/", "/clients"]), Some(1));
        assert_eq!(active_tab("/other", &["/", "/clients"]), None);
    }
}
