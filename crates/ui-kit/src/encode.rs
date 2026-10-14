/// Кодирует значение параметра запроса: без изменений остаются только безопасные символы.
pub fn encode_query_value(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::encode_query_value;

    #[test]
    fn keeps_unreserved_and_escapes_the_rest() {
        assert_eq!(encode_query_value("abc-1_2.3~"), "abc-1_2.3~");
        assert_eq!(encode_query_value("a b&c=d%"), "a%20b%26c%3Dd%25");
        assert_eq!(encode_query_value("иван"), "%D0%B8%D0%B2%D0%B0%D0%BD");
    }
}
