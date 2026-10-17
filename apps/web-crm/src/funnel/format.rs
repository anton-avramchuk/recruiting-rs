//! Форматирование значений воронки: общие форматы живут в `ui-core`.
pub use ui_kit::format::{
    initials, local_to_rfc3339, offset_suffix, rfc_midnight, short_moment, time_label,
};

/// Срок открытия вакансии считается давним после 45 дней.
pub const LONG_OPEN_DAYS: i64 = 45;

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
    fn triage_estimate_is_six_seconds_each_but_at_least_a_minute() {
        assert_eq!(triage_minutes(0), 1);
        assert_eq!(triage_minutes(3), 1);
        assert_eq!(triage_minutes(10), 1);
        assert_eq!(triage_minutes(11), 2);
        assert_eq!(triage_minutes(100), 10);
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
