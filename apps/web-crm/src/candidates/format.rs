//! Форматирование значений кандидатов: общие форматы живут в `ui-core`.
pub use ui_kit::format::{format_salary, parse_optional_number, years_label};

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
    fn skills_split_on_commas_and_newlines() {
        assert_eq!(
            split_skills("Rust, SQL;\n Docker ,, "),
            ["Rust", "SQL", "Docker"]
        );
        assert!(split_skills("  ").is_empty());
    }
}
