//! Выбор строк и курсор таблицы: чистая логика без разметки.

/// Выбранные строки (по идентификаторам) в порядке выбора.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    ids: Vec<String>,
}

impl Selection {
    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.ids.iter().any(|s| s == id)
    }

    /// Переключает одну строку.
    pub fn toggle(&mut self, id: &str) {
        if let Some(pos) = self.ids.iter().position(|s| s == id) {
            self.ids.remove(pos);
        } else {
            self.ids.push(id.to_string());
        }
    }

    /// Добавляет все перечисленные строки (уже выбранные остаются на месте).
    pub fn select_all<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>) {
        for id in ids {
            if !self.contains(id) {
                self.ids.push(id.to_string());
            }
        }
    }

    /// Снимает выбор со всех строк, перечисленных в `ids`.
    pub fn deselect_all<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>) {
        let drop: Vec<&str> = ids.into_iter().collect();
        self.ids.retain(|s| !drop.contains(&s.as_str()));
    }

    pub fn clear(&mut self) {
        self.ids.clear();
    }

    /// Все ли перечисленные строки выбраны (для общего флажка). Для пустого перечня - `false`.
    pub fn covers<'a>(&self, ids: impl IntoIterator<Item = &'a str>) -> bool {
        let mut any = false;
        for id in ids {
            any = true;
            if !self.contains(id) {
                return false;
            }
        }
        any
    }
}

/// Новая позиция курсора после шага `delta` (±1): на границах остаётся, без курсора - первая строка.
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

    #[test]
    fn toggling_adds_and_removes_in_order() {
        let mut s = Selection::default();
        s.toggle("a");
        s.toggle("b");
        assert_eq!(s.ids(), ["a", "b"]);
        s.toggle("a");
        assert_eq!(s.ids(), ["b"]);
        assert!(s.contains("b") && !s.contains("a"));
    }

    #[test]
    fn select_all_keeps_existing_order_without_duplicates() {
        let mut s = Selection::default();
        s.toggle("b");
        s.select_all(["a", "b", "c"]);
        assert_eq!(s.ids(), ["b", "a", "c"]);
    }

    #[test]
    fn deselect_all_drops_only_the_listed_rows() {
        let mut s = Selection::default();
        s.select_all(["a", "b", "c"]);
        s.deselect_all(["a", "c", "x"]);
        assert_eq!(s.ids(), ["b"]);
        s.clear();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn the_header_checkbox_is_on_only_when_every_visible_row_is_selected() {
        let mut s = Selection::default();
        s.select_all(["a", "b"]);
        assert!(s.covers(["a", "b"]));
        assert!(!s.covers(["a", "b", "c"]));
        assert!(!s.covers(std::iter::empty::<&str>()));
    }

    #[test]
    fn the_cursor_moves_and_stops_at_the_edges() {
        assert_eq!(move_cursor(None, 3, 1), Some(0));
        assert_eq!(move_cursor(None, 3, -1), Some(0));
        assert_eq!(move_cursor(Some(0), 3, -1), Some(0));
        assert_eq!(move_cursor(Some(1), 3, 1), Some(2));
        assert_eq!(move_cursor(Some(2), 3, 1), Some(2));
        assert_eq!(move_cursor(Some(0), 0, 1), None);
    }

    #[test]
    fn the_cursor_survives_a_shorter_list() {
        assert_eq!(clamp_cursor(Some(5), 3), Some(2));
        assert_eq!(clamp_cursor(Some(1), 3), Some(1));
        assert_eq!(clamp_cursor(Some(1), 0), None);
        assert_eq!(clamp_cursor(None, 3), None);
    }
}
