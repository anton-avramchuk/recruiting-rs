//! Состояние команды над сущностью: идёт ли она и чем закончилась. Без реактивности:
//! `ui-data::Mutation` кладёт этот трекер в сигнал, а проверяется он обычными юнит-тестами.
use crate::Reply;

/// Чем закончилась команда.
#[derive(Debug, Clone, PartialEq)]
pub enum Finished<T> {
    /// Успех с данными ответа.
    Success(T),
    /// Неудача с текстом для пользователя.
    Failure(String),
}

/// Трекер команды: не даёт запустить вторую, пока идёт первая.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tracker {
    pending: bool,
}

impl Tracker {
    pub fn is_pending(&self) -> bool {
        self.pending
    }

    /// Начинает команду; `false`, если предыдущая ещё не завершилась (запуск игнорируется).
    pub fn begin(&mut self) -> bool {
        if self.pending {
            return false;
        }
        self.pending = true;
        true
    }

    /// Завершает команду. `transport` - ответ сервера либо `Err`, если вызов не дошёл (сеть, сбой);
    /// `fallback` - текст на этот случай, `conflict` переводит текст конфликта.
    pub fn finish<T>(
        &mut self,
        transport: Result<Reply<T>, ()>,
        fallback: &str,
        conflict: impl Fn(&str) -> String,
    ) -> Finished<T> {
        self.pending = false;
        match transport {
            Ok(Reply::Ok(value)) => Finished::Success(value),
            Ok(other) => Finished::Failure(
                other
                    .failure_with(conflict)
                    .unwrap_or_else(|| fallback.to_string()),
            ),
            Err(()) => Finished::Failure(fallback.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(m: &str) -> String {
        m.to_string()
    }

    #[test]
    fn a_new_tracker_is_idle_and_begin_marks_it_pending() {
        let mut t = Tracker::default();
        assert!(!t.is_pending());
        assert!(t.begin());
        assert!(t.is_pending());
    }

    #[test]
    fn a_second_start_is_ignored_until_the_first_finishes() {
        let mut t = Tracker::default();
        assert!(t.begin());
        assert!(!t.begin());
        t.finish(Ok(Reply::Ok(1)), "сбой", same);
        assert!(!t.is_pending());
        assert!(t.begin());
    }

    #[test]
    fn a_success_carries_the_data() {
        let mut t = Tracker::default();
        t.begin();
        assert_eq!(
            t.finish(Ok(Reply::Ok(5)), "сбой", same),
            Finished::Success(5)
        );
    }

    #[test]
    fn a_reply_failure_gives_its_message() {
        let mut t = Tracker::default();
        t.begin();
        assert_eq!(
            t.finish::<u8>(Ok(Reply::Forbidden), "сбой", same),
            Finished::Failure("Недостаточно прав для этого действия.".into())
        );
    }

    #[test]
    fn a_conflict_is_translated_by_the_caller() {
        let mut t = Tracker::default();
        t.begin();
        let done = t.finish::<u8>(
            Ok(Reply::Conflict("name already in use".into())),
            "сбой",
            |_| "Такое название уже есть.".into(),
        );
        assert_eq!(done, Finished::Failure("Такое название уже есть.".into()));
    }

    #[test]
    fn a_transport_failure_uses_the_fallback_text() {
        let mut t = Tracker::default();
        t.begin();
        assert_eq!(
            t.finish::<u8>(Err(()), "Не удалось сохранить.", same),
            Finished::Failure("Не удалось сохранить.".into())
        );
        assert!(!t.is_pending());
    }
}
