//! Расчёт страниц для пейджера.

/// Число страниц при общем числе записей и размере страницы (не меньше одной).
pub fn page_count(total: u64, page_size: u32) -> u32 {
    if page_size == 0 {
        return 1;
    }
    total.div_ceil(u64::from(page_size)).max(1) as u32
}

/// Подпись «Страница N из M» для списков без общего диапазона.
pub fn page_label(page: u32, pages: u32) -> String {
    format!("Страница {page} из {pages}")
}

/// Можно ли перейти назад с этой страницы.
pub fn has_prev(page: u32) -> bool {
    page > 1
}

/// Можно ли перейти вперёд (страниц `pages`, текущая `page`).
pub fn has_next(page: u32, pages: u32) -> bool {
    page < pages
}

/// Страница, приведённая к допустимым границам `1..=pages`.
pub fn clamp_page(page: u32, pages: u32) -> u32 {
    page.clamp(1, pages.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_count_rounds_up_and_is_at_least_one() {
        assert_eq!(page_count(0, 20), 1);
        assert_eq!(page_count(1, 20), 1);
        assert_eq!(page_count(20, 20), 1);
        assert_eq!(page_count(21, 20), 2);
        assert_eq!(page_count(135, 20), 7);
        assert_eq!(page_count(10, 0), 1);
    }

    #[test]
    fn the_label_names_the_page_and_the_total() {
        assert_eq!(page_label(2, 7), "Страница 2 из 7");
        assert_eq!(page_label(1, 1), "Страница 1 из 1");
    }

    #[test]
    fn the_buttons_are_blocked_on_the_borders() {
        assert!(!has_prev(1));
        assert!(has_prev(2));
        assert!(has_next(1, 2));
        assert!(!has_next(2, 2));
        assert!(!has_next(1, 1));
    }

    #[test]
    fn a_page_outside_the_range_is_pulled_back() {
        assert_eq!(clamp_page(0, 5), 1);
        assert_eq!(clamp_page(9, 5), 5);
        assert_eq!(clamp_page(3, 5), 3);
        assert_eq!(clamp_page(3, 0), 1);
    }
}
