//! Клавиши списка откликов и панели человека: чистый разбор (без DOM).
use crate::palette::keys::Press;

/// Число этапов воронки на клавишах `1…8`.
pub const STAGE_KEYS: u8 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKey {
    Next,
    Prev,
    Open,
    Toggle,
    SelectAll,
    /// Номер этапа, 1…8.
    Stage(u8),
    Advance,
    Reject,
    Letter,
}

impl ListKey {
    /// Меняет ли действие данные (недоступно роли `viewer`).
    pub fn needs_write(self) -> bool {
        !matches!(self, ListKey::Next | ListKey::Prev | ListKey::Open)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelKey {
    Advance,
    Reject,
    Interview,
    Note,
    /// Номер этапа, 1…8.
    Stage(u8),
}

/// Номер этапа по клавише: верхний ряд, цифровая клавиатура или символ.
fn stage_number(p: &Press<'_>) -> Option<u8> {
    let digit = p
        .code
        .strip_prefix("Digit")
        .or_else(|| p.code.strip_prefix("Numpad"))
        .or(Some(p.key))
        .filter(|d| d.len() == 1)?
        .parse::<u8>()
        .ok()?;
    (1..=STAGE_KEYS).contains(&digit).then_some(digit)
}

pub fn list_key(p: Press<'_>) -> Option<ListKey> {
    if p.key == "Enter" {
        return Some(ListKey::Open);
    }
    if let Some(n) = stage_number(&p) {
        return Some(ListKey::Stage(n));
    }
    Some(match p.code {
        "KeyJ" => ListKey::Next,
        "KeyK" => ListKey::Prev,
        "KeyX" => ListKey::Toggle,
        "KeyA" => ListKey::SelectAll,
        "KeyE" => ListKey::Advance,
        "KeyR" => ListKey::Reject,
        "KeyM" => ListKey::Letter,
        _ => return None,
    })
}

pub fn panel_key(p: Press<'_>) -> Option<PanelKey> {
    if let Some(n) = stage_number(&p) {
        return Some(PanelKey::Stage(n));
    }
    Some(match p.code {
        "KeyE" => PanelKey::Advance,
        "KeyR" => PanelKey::Reject,
        "KeyS" => PanelKey::Interview,
        "KeyN" => PanelKey::Note,
        _ => return None,
    })
}

/// Новая позиция курсора после шага `delta` (±1): на границах остаётся, без курсора — первая строка.
pub fn move_cursor(current: Option<usize>, len: usize, delta: i32) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    Some(match current {
        None => 0,
        Some(i) if delta < 0 => i.saturating_sub(1).min(last),
        Some(i) => (i + 1).min(last),
    })
}

/// Курсор после обновления списка: остаётся на том же месте, но не выходит за список.
pub fn clamp_cursor(current: Option<usize>, len: usize) -> Option<usize> {
    match (current, len) {
        (_, 0) => None,
        (Some(i), n) => Some(i.min(n - 1)),
        (None, _) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p<'a>(key: &'a str, code: &'a str) -> Press<'a> {
        Press {
            key,
            code,
            shift: false,
        }
    }

    #[test]
    fn list_letters_map_by_physical_key() {
        for (code, key) in [
            ("KeyJ", ListKey::Next),
            ("KeyK", ListKey::Prev),
            ("KeyX", ListKey::Toggle),
            ("KeyA", ListKey::SelectAll),
            ("KeyE", ListKey::Advance),
            ("KeyR", ListKey::Reject),
            ("KeyM", ListKey::Letter),
        ] {
            assert_eq!(list_key(p("я", code)), Some(key), "{code}");
        }
        assert_eq!(list_key(p("Enter", "Enter")), Some(ListKey::Open));
        assert_eq!(list_key(p("z", "KeyZ")), None);
        // S и N принадлежат панели, в списке их нет
        assert_eq!(list_key(p("s", "KeyS")), None);
        assert_eq!(list_key(p("n", "KeyN")), None);
    }

    #[test]
    fn digits_one_to_eight_pick_a_stage_from_any_keyboard() {
        assert_eq!(list_key(p("3", "Digit3")), Some(ListKey::Stage(3)));
        assert_eq!(list_key(p("8", "Numpad8")), Some(ListKey::Stage(8)));
        assert_eq!(list_key(p("1", "")), Some(ListKey::Stage(1)));
        assert_eq!(list_key(p("0", "Digit0")), None);
        assert_eq!(list_key(p("9", "Digit9")), None);
        assert_eq!(panel_key(p("5", "Digit5")), Some(PanelKey::Stage(5)));
        assert_eq!(panel_key(p("9", "Digit9")), None);
    }

    #[test]
    fn panel_keys_cover_the_actions() {
        for (code, key) in [
            ("KeyE", PanelKey::Advance),
            ("KeyR", PanelKey::Reject),
            ("KeyS", PanelKey::Interview),
            ("KeyN", PanelKey::Note),
        ] {
            assert_eq!(panel_key(p("x", code)), Some(key), "{code}");
        }
        assert_eq!(panel_key(p("j", "KeyJ")), None);
        assert_eq!(panel_key(p("Enter", "Enter")), None);
    }

    #[test]
    fn only_moving_and_opening_are_read_only() {
        for key in [ListKey::Next, ListKey::Prev, ListKey::Open] {
            assert!(!key.needs_write());
        }
        for key in [
            ListKey::Toggle,
            ListKey::SelectAll,
            ListKey::Stage(2),
            ListKey::Advance,
            ListKey::Reject,
            ListKey::Letter,
        ] {
            assert!(key.needs_write(), "{key:?}");
        }
    }

    #[test]
    fn the_cursor_moves_and_stops_at_the_edges() {
        assert_eq!(move_cursor(None, 3, 1), Some(0));
        assert_eq!(move_cursor(None, 3, -1), Some(0));
        assert_eq!(move_cursor(Some(0), 3, 1), Some(1));
        assert_eq!(move_cursor(Some(1), 3, 1), Some(2));
        assert_eq!(move_cursor(Some(2), 3, 1), Some(2));
        assert_eq!(move_cursor(Some(1), 3, -1), Some(0));
        assert_eq!(move_cursor(Some(0), 3, -1), Some(0));
        assert_eq!(move_cursor(Some(5), 3, 1), Some(2));
        assert_eq!(move_cursor(Some(0), 0, 1), None);
        assert_eq!(move_cursor(None, 0, 1), None);
    }

    #[test]
    fn the_cursor_survives_a_shorter_list() {
        assert_eq!(clamp_cursor(Some(4), 3), Some(2));
        assert_eq!(clamp_cursor(Some(1), 3), Some(1));
        assert_eq!(clamp_cursor(Some(1), 0), None);
        assert_eq!(clamp_cursor(None, 3), None);
    }
}
