//! Состояние поля формы без разметки: значение, «тронуто» и ошибка проверки.

/// Поле формы. Ошибка показывается только после того, как поле «тронули»
/// (покинули или отправили форму), чтобы не ругаться на ещё не набранное.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormControl {
    value: String,
    touched: bool,
    error: Option<String>,
}

impl FormControl {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            ..Self::default()
        }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    /// Новое значение. Ошибка прежней проверки снимается: поле проверят заново.
    pub fn set(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.error = None;
    }

    /// Поле тронули.
    pub fn touch(&mut self) {
        self.touched = true;
    }

    pub fn is_touched(&self) -> bool {
        self.touched
    }

    /// Проверяет значение правилом; `Some(текст)` - ошибка.
    pub fn validate(&mut self, rule: impl Fn(&str) -> Option<String>) -> bool {
        self.error = rule(&self.value);
        self.error.is_none()
    }

    /// Ошибка с сервера (например, занятое название).
    pub fn fail(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
        self.touched = true;
    }

    /// Ошибка для показа: только у тронутого поля.
    pub fn shown_error(&self) -> Option<&str> {
        self.touched.then_some(self.error.as_deref()).flatten()
    }

    pub fn is_valid(&self) -> bool {
        self.error.is_none()
    }

    /// Возвращает поле к исходному виду с новым значением.
    pub fn reset(&mut self, value: impl Into<String>) {
        *self = Self::new(value);
    }
}

/// Правило «обязательно»: пустое значение (после обрезки) - ошибка.
pub fn required(message: &'static str) -> impl Fn(&str) -> Option<String> {
    move |v| v.trim().is_empty().then(|| message.to_string())
}

/// Правило «не длиннее N символов».
pub fn max_chars(limit: usize, message: &'static str) -> impl Fn(&str) -> Option<String> {
    move |v| (v.chars().count() > limit).then(|| message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_control_holds_its_value_untouched_and_valid() {
        let c = FormControl::new("Acme");
        assert_eq!(c.value(), "Acme");
        assert!(!c.is_touched());
        assert!(c.is_valid());
        assert_eq!(c.shown_error(), None);
    }

    #[test]
    fn an_error_is_hidden_until_the_field_is_touched() {
        let mut c = FormControl::new("");
        assert!(!c.validate(required("Укажите название")));
        assert_eq!(c.shown_error(), None);
        c.touch();
        assert_eq!(c.shown_error(), Some("Укажите название"));
    }

    #[test]
    fn typing_clears_the_old_error_and_validation_can_pass() {
        let mut c = FormControl::new("");
        c.touch();
        c.validate(required("Укажите название"));
        c.set("Acme");
        assert_eq!(c.shown_error(), None);
        assert!(c.validate(required("Укажите название")));
        assert!(c.is_valid());
    }

    #[test]
    fn a_server_error_is_shown_at_once() {
        let mut c = FormControl::new("acme");
        c.fail("Такое название уже есть");
        assert!(c.is_touched());
        assert!(!c.is_valid());
        assert_eq!(c.shown_error(), Some("Такое название уже есть"));
    }

    #[test]
    fn rules_trim_and_count_characters() {
        assert!(required("x")("   ").is_some());
        assert!(required("x")(" a ").is_none());
        assert!(max_chars(3, "длинно")("яяяя").is_some());
        assert!(max_chars(3, "длинно")("яяя").is_none());
    }

    #[test]
    fn reset_returns_the_control_to_its_initial_state() {
        let mut c = FormControl::new("a");
        c.fail("err");
        c.reset("b");
        assert_eq!(c, FormControl::new("b"));
    }
}
