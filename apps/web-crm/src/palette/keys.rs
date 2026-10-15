//! Горячие клавиши оболочки: чистый разбор нажатий (без DOM).

/// Окно для второй клавиши последовательности `G + буква`, мс.
pub const SEQUENCE_MS: f64 = 1000.0;

/// Нажатие в том виде, который нужен разбору. Буквы сравниваются по `code`,
/// чтобы сочетания не зависели от раскладки.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Press<'a> {
    pub key: &'a str,
    pub code: &'a str,
    pub shift: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    Go(&'static str),
    /// Фокус в строку поиска списка кандидатов.
    Search,
    Import,
    Triage,
    Cheatsheet,
}

impl Shortcut {
    /// Меняет ли действие данные (недоступно роли `viewer`).
    pub fn needs_write(self) -> bool {
        matches!(self, Shortcut::Import | Shortcut::Triage)
    }
}

/// Состояние последовательности: когда нажата `G`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sequence {
    g_at: Option<f64>,
}

fn go_target(code: &str) -> Option<&'static str> {
    Some(match code {
        "KeyT" => "/",
        "KeyV" => "/vacancies",
        "KeyO" => "/applications",
        "KeyC" => "/candidates",
        "KeyA" => "/analytics",
        _ => return None,
    })
}

impl Sequence {
    /// Разбирает нажатие в момент `now` (мс). Вторая клавиша вне окна последовательности
    /// или неподходящая сбрасывает её и обрабатывается как обычная.
    pub fn press(&mut self, p: Press<'_>, now: f64) -> Option<Shortcut> {
        if let Some(at) = self.g_at.take()
            && now - at <= SEQUENCE_MS
            && let Some(path) = go_target(p.code)
        {
            return Some(Shortcut::Go(path));
        }
        if p.key == "?" {
            return Some(Shortcut::Cheatsheet);
        }
        match p.code {
            "KeyG" => {
                self.g_at = Some(now);
                None
            }
            "KeyI" => Some(Shortcut::Import),
            "KeyT" => Some(Shortcut::Triage),
            "Slash" if p.shift => Some(Shortcut::Cheatsheet),
            "Slash" => Some(Shortcut::Search),
            // цифровая клавиатура и раскладки, где «/» не на клавише Slash
            _ if p.key == "/" => Some(Shortcut::Search),
            _ => None,
        }
    }

    /// Сбрасывает ожидание второй клавиши (окно открылось, фокус ушёл в поле).
    pub fn reset(&mut self) {
        self.g_at = None;
    }
}

/// Шпаргалка: сочетание (клавиши по одной) и описание. Только реализованное.
pub const CHEATSHEET: &[(&[&str], &str)] = &[
    (&["Ctrl", "K"], "Палитра команд — поиск по всему"),
    (&["G", "T"], "Сегодня"),
    (&["G", "V"], "Вакансии и воронка"),
    (&["G", "O"], "Отклики"),
    (&["G", "C"], "База кандидатов"),
    (&["G", "A"], "Аналитика"),
    (&["/"], "Фокус в поиск по кандидатам"),
    (&["T"], "Режим разбора откликов"),
    (&["I"], "Импорт из файла"),
    (&["?"], "Эта шпаргалка"),
    (&["Esc"], "Закрыть окно"),
];

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
    fn g_then_a_letter_goes_to_a_section() {
        let mut s = Sequence::default();
        assert_eq!(s.press(p("g", "KeyG"), 0.0), None);
        assert_eq!(
            s.press(p("c", "KeyC"), 300.0),
            Some(Shortcut::Go("/candidates"))
        );
        for (code, path) in [
            ("KeyT", "/"),
            ("KeyV", "/vacancies"),
            ("KeyO", "/applications"),
            ("KeyA", "/analytics"),
        ] {
            s.press(p("g", "KeyG"), 0.0);
            assert_eq!(s.press(p("x", code), 10.0), Some(Shortcut::Go(path)));
        }
    }

    #[test]
    fn a_late_second_key_is_not_a_jump() {
        let mut s = Sequence::default();
        s.press(p("g", "KeyG"), 0.0);
        assert_eq!(s.press(p("c", "KeyC"), SEQUENCE_MS + 1.0), None);
        // и сама последовательность сброшена
        assert_eq!(s.press(p("c", "KeyC"), SEQUENCE_MS + 2.0), None);
    }

    #[test]
    fn the_second_key_is_taken_within_the_window_inclusive() {
        let mut s = Sequence::default();
        s.press(p("g", "KeyG"), 0.0);
        assert_eq!(
            s.press(p("a", "KeyA"), SEQUENCE_MS),
            Some(Shortcut::Go("/analytics"))
        );
    }

    #[test]
    fn a_wrong_second_key_resets_and_acts_as_a_normal_key() {
        let mut s = Sequence::default();
        s.press(p("g", "KeyG"), 0.0);
        assert_eq!(s.press(p("i", "KeyI"), 100.0), Some(Shortcut::Import));
        assert_eq!(s.press(p("c", "KeyC"), 150.0), None);
    }

    #[test]
    fn a_reset_forgets_the_first_key() {
        let mut s = Sequence::default();
        s.press(p("g", "KeyG"), 0.0);
        s.reset();
        assert_eq!(s.press(p("c", "KeyC"), 10.0), None);
    }

    #[test]
    fn single_keys_map_to_shortcuts() {
        let mut s = Sequence::default();
        assert_eq!(s.press(p("t", "KeyT"), 0.0), Some(Shortcut::Triage));
        assert_eq!(s.press(p("i", "KeyI"), 0.0), Some(Shortcut::Import));
        assert_eq!(s.press(p("/", "Slash"), 0.0), Some(Shortcut::Search));
        assert_eq!(
            s.press(
                Press {
                    key: "?",
                    code: "Slash",
                    shift: true
                },
                0.0
            ),
            Some(Shortcut::Cheatsheet)
        );
        assert_eq!(s.press(p("z", "KeyZ"), 0.0), None);
    }

    #[test]
    fn a_slash_from_the_numpad_also_searches() {
        let mut s = Sequence::default();
        assert_eq!(s.press(p("/", "NumpadDivide"), 0.0), Some(Shortcut::Search));
    }

    #[test]
    fn letters_work_in_any_layout() {
        let mut s = Sequence::default();
        // русская раскладка: key «е», физическая клавиша T
        assert_eq!(s.press(p("е", "KeyT"), 0.0), Some(Shortcut::Triage));
        s.press(p("п", "KeyG"), 0.0);
        assert_eq!(
            s.press(p("с", "KeyC"), 5.0),
            Some(Shortcut::Go("/candidates"))
        );
    }

    #[test]
    fn a_question_mark_on_another_key_opens_the_cheatsheet() {
        let mut s = Sequence::default();
        let press = Press {
            key: "?",
            code: "Digit7",
            shift: true,
        };
        assert_eq!(s.press(press, 0.0), Some(Shortcut::Cheatsheet));
    }

    #[test]
    fn only_data_changing_shortcuts_need_write_access() {
        assert!(Shortcut::Import.needs_write() && Shortcut::Triage.needs_write());
        assert!(!Shortcut::Search.needs_write() && !Shortcut::Cheatsheet.needs_write());
        assert!(!Shortcut::Go("/").needs_write());
    }

    #[test]
    fn the_cheatsheet_has_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for (keys, text) in CHEATSHEET {
            assert!(!text.is_empty() && !keys.is_empty());
            assert!(seen.insert(keys.join("+")), "{keys:?}");
        }
    }
}
