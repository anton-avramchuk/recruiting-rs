//! Комментарии к заказчикам, вакансиям и кандидатам.
pub mod server;
pub mod view;

pub use view::Comments;

/// Права на действия с комментарием по данным сессии и самого комментария.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rights {
    pub edit: bool,
    pub delete: bool,
}

/// Править может только автор, удалять - автор и владелец; без права записи - ничего.
pub fn rights(can_write: bool, is_owner: bool, me: &str, author: &str) -> Rights {
    let mine = !me.is_empty() && me == author;
    Rights {
        edit: can_write && mine,
        delete: can_write && (mine || is_owner),
    }
}

/// Сколько записей грузить после действия: всё уже показанное и одну запись сверху.
pub fn reload_limit(shown: usize) -> i64 {
    (shown as i64 + 1).clamp(50, 200)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_author_edits_and_deletes_the_owner_only_deletes() {
        assert_eq!(
            rights(true, false, "u1", "u1"),
            Rights {
                edit: true,
                delete: true
            }
        );
        assert_eq!(
            rights(true, false, "u1", "u2"),
            Rights {
                edit: false,
                delete: false
            }
        );
        assert_eq!(
            rights(true, true, "u1", "u2"),
            Rights {
                edit: false,
                delete: true
            }
        );
    }

    #[test]
    fn a_viewer_and_an_unknown_user_get_nothing() {
        assert_eq!(
            rights(false, true, "u1", "u1"),
            Rights {
                edit: false,
                delete: false
            }
        );
        assert_eq!(
            rights(true, true, "", ""),
            Rights {
                edit: false,
                delete: true
            }
        );
        assert!(!rights(true, false, "", "").edit);
    }

    #[test]
    fn reload_covers_what_is_shown_within_the_page_limits() {
        assert_eq!(reload_limit(0), 50);
        assert_eq!(reload_limit(49), 50);
        assert_eq!(reload_limit(120), 121);
        assert_eq!(reload_limit(500), 200);
    }
}
