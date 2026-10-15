//! Форматирование значений воронки для отображения.

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

/// Срок открытия вакансии считается давним после 45 дней.
pub const LONG_OPEN_DAYS: i64 = 45;

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

/// Часовой пояс браузера: минуты «UTC - местное».
#[cfg(feature = "hydrate")]
pub fn browser_timezone_offset() -> i32 {
    js_sys::Date::new_0().get_timezone_offset() as i32
}

#[cfg(not(feature = "hydrate"))]
pub fn browser_timezone_offset() -> i32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
