//! Форматирование значений для отображения.

/// «1 год», «2 года», «5 лет» (11-14 всегда «лет»).
pub fn years_label(n: i32) -> String {
    let n_abs = n.abs();
    let word = match (n_abs % 100, n_abs % 10) {
        (11..=14, _) => "лет",
        (_, 1) => "год",
        (_, 2..=4) => "года",
        _ => "лет",
    };
    format!("{n} {word}")
}

/// «250 000 ₽»: разряды разделены неразрывным пробелом.
pub fn format_salary(amount: i64) -> String {
    let digits = amount.abs().to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push('\u{a0}');
        }
        grouped.push(c);
    }
    let sign = if amount < 0 { "-" } else { "" };
    format!("{sign}{grouped}\u{a0}₽")
}

/// Текст в число для необязательных числовых полей формы; пусто или мусор - `None`.
pub fn parse_optional_number(raw: &str) -> Option<i64> {
    let cleaned: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    cleaned.parse().ok()
}

/// Навыки из строки через запятую (или перевод строки).
pub fn split_skills(raw: &str) -> Vec<String> {
    raw.split([',', '\n', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn years_use_the_right_russian_form() {
        let cases = [
            (0, "0 лет"),
            (1, "1 год"),
            (2, "2 года"),
            (4, "4 года"),
            (5, "5 лет"),
            (11, "11 лет"),
            (12, "12 лет"),
            (14, "14 лет"),
            (21, "21 год"),
            (22, "22 года"),
            (25, "25 лет"),
            (111, "111 лет"),
        ];
        for (n, expected) in cases {
            assert_eq!(years_label(n), expected, "{n}");
        }
    }

    #[test]
    fn salary_is_grouped_by_thousands() {
        assert_eq!(format_salary(250_000), "250\u{a0}000\u{a0}₽");
        assert_eq!(format_salary(1_500_000), "1\u{a0}500\u{a0}000\u{a0}₽");
        assert_eq!(format_salary(900), "900\u{a0}₽");
        assert_eq!(format_salary(0), "0\u{a0}₽");
    }

    #[test]
    fn numbers_tolerate_spaces_and_reject_garbage() {
        assert_eq!(parse_optional_number(" 250 000 "), Some(250_000));
        assert_eq!(parse_optional_number(""), None);
        assert_eq!(parse_optional_number("много"), None);
    }

    #[test]
    fn skills_split_on_commas_and_newlines() {
        assert_eq!(
            split_skills("Rust, SQL;\n Docker ,, "),
            ["Rust", "SQL", "Docker"]
        );
        assert!(split_skills("  ").is_empty());
    }
}
