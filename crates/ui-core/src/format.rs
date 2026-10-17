//! Форматирование значений для отображения (русская локаль) и разбор ввода. Без браузера.

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

/// Время на этапе: «5 ч» до двух суток, дальше «3 д».
pub fn time_label(hours: i64) -> String {
    if hours < 48 {
        format!("{} ч", hours.max(0))
    } else {
        format!("{} д", hours / 24)
    }
}

/// «15.10 12:00» из RFC 3339 (по UTC, как отдаёт сервер).
pub fn short_moment(rfc3339: &str) -> String {
    let date = rfc3339.get(..10).unwrap_or_default();
    let time = rfc3339.get(11..16).unwrap_or_default();
    match (date.get(8..10), date.get(5..7)) {
        (Some(day), Some(month)) if !time.is_empty() => format!("{day}.{month} {time}"),
        _ => rfc3339.to_string(),
    }
}

/// Смещение часового пояса в виде `+03:00` по минутам «UTC - местное» (как у `getTimezoneOffset`).
pub fn offset_suffix(timezone_offset_minutes: i32) -> String {
    let east = -timezone_offset_minutes;
    let sign = if east < 0 { '-' } else { '+' };
    let abs = east.abs();
    format!("{sign}{:02}:{:02}", abs / 60, abs % 60)
}

/// Значение поля `datetime-local` (`2026-10-20T15:00`) и пояс -> RFC 3339.
pub fn local_to_rfc3339(local: &str, timezone_offset_minutes: i32) -> Option<String> {
    let local = local.trim();
    if local.len() < 16 || local.get(10..11) != Some("T") {
        return None;
    }
    let seconds = if local.len() == 16 { ":00" } else { "" };
    Some(format!(
        "{local}{seconds}{}",
        offset_suffix(timezone_offset_minutes)
    ))
}

/// Полночь указанной даты в часовом поясе пользователя (`месяц` с единицы).
pub fn rfc_midnight(year: i32, month: u32, day: u32, timezone_offset_minutes: i32) -> String {
    format!(
        "{year:04}-{month:02}-{day:02}T00:00:00{}",
        offset_suffix(timezone_offset_minutes)
    )
}

/// Инициалы для кружка рядом с именем: первые буквы первых двух слов.
pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .filter_map(|w| w.chars().next())
        .flat_map(char::to_uppercase)
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
    fn time_is_hours_then_days() {
        assert_eq!(time_label(0), "0 ч");
        assert_eq!(time_label(30), "30 ч");
        assert_eq!(time_label(47), "47 ч");
        assert_eq!(time_label(48), "2 д");
        assert_eq!(time_label(200), "8 д");
        assert_eq!(time_label(-3), "0 ч");
    }

    #[test]
    fn moment_is_day_month_and_time() {
        assert_eq!(short_moment("2026-10-15T12:34:56+00:00"), "15.10 12:34");
        assert_eq!(short_moment("мусор"), "мусор");
    }

    #[test]
    fn offsets_follow_the_javascript_sign_convention() {
        assert_eq!(offset_suffix(-180), "+03:00");
        assert_eq!(offset_suffix(0), "+00:00");
        assert_eq!(offset_suffix(300), "-05:00");
        assert_eq!(offset_suffix(-330), "+05:30");
    }

    #[test]
    fn datetime_local_gets_seconds_and_offset() {
        assert_eq!(
            local_to_rfc3339("2026-10-20T15:00", -180).as_deref(),
            Some("2026-10-20T15:00:00+03:00")
        );
        assert_eq!(
            local_to_rfc3339("2026-10-20T15:00:30", 0).as_deref(),
            Some("2026-10-20T15:00:30+00:00")
        );
        assert_eq!(local_to_rfc3339("", 0), None);
        assert_eq!(local_to_rfc3339("завтра в 3", 0), None);
    }

    #[test]
    fn midnight_carries_the_user_offset() {
        assert_eq!(rfc_midnight(2026, 3, 9, -180), "2026-03-09T00:00:00+03:00");
        assert_eq!(rfc_midnight(2026, 12, 1, 300), "2026-12-01T00:00:00-05:00");
        assert_eq!(rfc_midnight(2026, 1, 1, 0), "2026-01-01T00:00:00+00:00");
    }

    #[test]
    fn initials_use_the_first_two_words() {
        assert_eq!(initials("анна смирнова"), "АС");
        assert_eq!(initials("Борис"), "Б");
        assert_eq!(initials("  "), "");
        assert_eq!(initials("Иван Петрович Сидоров"), "ИП");
    }
}
