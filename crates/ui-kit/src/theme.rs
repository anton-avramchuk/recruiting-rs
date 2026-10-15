//! Светлая и тёмная тема: выбор запоминается в браузере, без выбора действует тема системы.

/// Ключ в `localStorage`.
pub const STORAGE_KEY: &str = "theme";

/// Ранний скрипт: ставит сохранённую тему до первой отрисовки. Ошибки хранилища игнорируются.
pub const EARLY_SCRIPT: &str = r#"try{var t=localStorage.getItem("theme");if(t==="light"||t==="dark")document.documentElement.dataset.theme=t}catch(e){}"#;

/// Тема, на которую переключит нажатие: противоположная действующей
/// (сохранённая, а без неё — системная).
pub fn next_theme(stored: Option<&str>, system_dark: bool) -> &'static str {
    let dark = match stored {
        Some("dark") => true,
        Some("light") => false,
        _ => system_dark,
    };
    if dark { "light" } else { "dark" }
}

/// Переключает тему в браузере и запоминает выбор. Возвращает новую тему.
#[cfg(feature = "hydrate")]
pub fn toggle_theme() -> Option<&'static str> {
    let window = web_sys::window()?;
    let root = window.document()?.document_element()?;
    let attr = root.get_attribute("data-theme");
    let system_dark = window
        .match_media("(prefers-color-scheme: dark)")
        .ok()
        .flatten()
        .is_some_and(|m| m.matches());
    let next = next_theme(attr.as_deref(), system_dark);
    root.set_attribute("data-theme", next).ok()?;
    // хранилище может быть недоступно: тема всё равно применена
    if let Ok(Some(storage)) = window.local_storage() {
        let _ = storage.set_item(STORAGE_KEY, next);
    }
    Some(next)
}

#[cfg(not(feature = "hydrate"))]
pub fn toggle_theme() -> Option<&'static str> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_choice_wins_over_the_system() {
        assert_eq!(next_theme(Some("dark"), false), "light");
        assert_eq!(next_theme(Some("light"), true), "dark");
    }

    #[test]
    fn without_a_choice_the_system_theme_is_current() {
        assert_eq!(next_theme(None, true), "light");
        assert_eq!(next_theme(None, false), "dark");
    }

    #[test]
    fn garbage_in_storage_counts_as_no_choice() {
        assert_eq!(next_theme(Some("sepia"), true), "light");
        assert_eq!(next_theme(Some(""), false), "dark");
    }

    #[test]
    fn toggling_twice_returns_to_the_start() {
        let first = next_theme(Some("light"), false);
        assert_eq!(next_theme(Some(first), false), "light");
    }

    #[test]
    fn the_early_script_reads_the_key_the_toggle_writes() {
        assert!(EARLY_SCRIPT.contains(&format!("\"{STORAGE_KEY}\"")));
        assert!(EARLY_SCRIPT.contains("try"));
    }
}
