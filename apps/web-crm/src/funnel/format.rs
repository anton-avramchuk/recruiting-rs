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

/// Заказчик и контакт вакансии одной строкой: «Acme · Мария Петрова, HR-директор».
pub fn client_line(
    client: Option<&str>,
    contact: Option<&str>,
    position: Option<&str>,
) -> Option<String> {
    let person = match (contact, position) {
        (Some(c), Some(p)) => Some(format!("{c}, {p}")),
        (Some(c), None) => Some(c.to_string()),
        _ => None,
    };
    let parts: Vec<String> = [client.map(str::to_string), person]
        .into_iter()
        .flatten()
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Текущий момент в миллисекундах (только в браузере; на сервере 0).
#[cfg(feature = "hydrate")]
pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(not(feature = "hydrate"))]
pub fn now_ms() -> f64 {
    0.0
}

/// Оценка времени разбора очереди: 6 секунд на отклик, но не меньше минуты.
pub fn triage_minutes(count: usize) -> i64 {
    ((count as i64 * 6 + 59) / 60).max(1)
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

/// Сегодняшние сутки по времени браузера: `[полночь, следующая полночь)`.
#[cfg(feature = "hydrate")]
pub fn local_day_range() -> Option<(String, String)> {
    let now = js_sys::Date::new_0();
    let (y, m, d) = (now.get_full_year() as i32, now.get_month(), now.get_date());
    let tomorrow = js_sys::Date::new_with_year_month_day(y as u32, m as i32, d as i32 + 1);
    let offset = browser_timezone_offset();
    Some((
        rfc_midnight(y, m + 1, d, offset),
        rfc_midnight(
            tomorrow.get_full_year() as i32,
            tomorrow.get_month() + 1,
            tomorrow.get_date(),
            offset,
        ),
    ))
}

#[cfg(not(feature = "hydrate"))]
pub fn local_day_range() -> Option<(String, String)> {
    None
}

/// «15:00» по времени браузера из RFC 3339.
#[cfg(feature = "hydrate")]
pub fn local_clock(rfc3339: &str) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_str(rfc3339));
    format!("{:02}:{:02}", date.get_hours(), date.get_minutes())
}

#[cfg(not(feature = "hydrate"))]
pub fn local_clock(rfc3339: &str) -> String {
    rfc3339.get(11..16).unwrap_or_default().to_string()
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

    #[test]
    fn triage_estimate_is_six_seconds_each_but_at_least_a_minute() {
        assert_eq!(triage_minutes(0), 1);
        assert_eq!(triage_minutes(3), 1);
        assert_eq!(triage_minutes(10), 1);
        assert_eq!(triage_minutes(11), 2);
        assert_eq!(triage_minutes(100), 10);
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

#[cfg(test)]
mod client_line_tests {
    use super::client_line;

    #[test]
    fn client_and_contact_are_joined_with_the_position() {
        assert_eq!(
            client_line(Some("Acme"), Some("Мария Петрова"), Some("HR-директор")).as_deref(),
            Some("Acme · Мария Петрова, HR-директор")
        );
        assert_eq!(
            client_line(Some("Acme"), Some("Мария"), None).as_deref(),
            Some("Acme · Мария")
        );
        assert_eq!(
            client_line(Some("Acme"), None, None).as_deref(),
            Some("Acme")
        );
        assert_eq!(client_line(None, None, Some("HR")), None);
        assert_eq!(
            client_line(None, Some("Мария"), None).as_deref(),
            Some("Мария")
        );
    }
}
