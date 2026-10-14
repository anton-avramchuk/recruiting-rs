//! Помощники для поиска `LIKE`/`ILIKE`.

/// Шаблон «содержит» с экранированием: `%`, `_` и `\` в запросе пользователя ищутся буквально.
/// В SQL используйте `ILIKE $n ESCAPE '\'`.
pub fn contains_pattern(query: &str) -> String {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

#[cfg(test)]
mod tests {
    use super::contains_pattern;

    #[test]
    fn wraps_the_query_in_wildcards() {
        assert_eq!(contains_pattern("anna"), "%anna%");
    }

    #[test]
    fn special_characters_lose_their_meaning() {
        assert_eq!(contains_pattern("100%"), "%100\\%%");
        assert_eq!(contains_pattern("a_b"), "%a\\_b%");
        assert_eq!(contains_pattern("c:\\x"), "%c:\\\\x%");
    }
}
