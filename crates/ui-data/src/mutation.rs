//! Команда над сущностью: запуск, признак «идёт», сообщение об ошибке, действие при успехе.
use std::{future::Future, sync::Arc};

use leptos::{prelude::*, task::spawn_local};
use ui_core::{
    Reply,
    mutation::{Finished, Tracker},
};

/// Настройки команды.
#[derive(Clone, Copy)]
pub struct MutationConfig {
    /// Текст, если вызов не дошёл до сервера.
    pub fallback: &'static str,
    /// Перевод текста конфликта.
    pub conflict: fn(&str) -> String,
    /// Общее место для сообщения экрана; по умолчанию команда заводит своё.
    pub message: Option<RwSignal<Option<String>>>,
}

impl Default for MutationConfig {
    fn default() -> Self {
        Self {
            fallback: "Не удалось выполнить действие. Попробуйте позже.",
            conflict: |m| m.to_string(),
            message: None,
        }
    }
}

/// Команда над сущностью. Повторный запуск, пока идёт предыдущий, игнорируется; при неудаче
/// сообщение попадает в `message`, при успехе вызывается действие и сообщение очищается.
pub struct Mutation<I: 'static> {
    pending: RwSignal<bool>,
    pub message: RwSignal<Option<String>>,
    start: StoredValue<Arc<dyn Fn(I) + Send + Sync>>,
}

impl<I> Clone for Mutation<I> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<I> Copy for Mutation<I> {}

impl<I: Send + Sync + 'static> Mutation<I> {
    /// `run` вызывает серверную функцию, `on_ok` получает данные успешного ответа.
    pub fn new<O, Fut>(
        run: impl Fn(I) -> Fut + Send + Sync + 'static,
        on_ok: impl Fn(O) + Send + Sync + 'static,
        config: MutationConfig,
    ) -> Self
    where
        O: 'static,
        Fut: Future<Output = Result<Reply<O>, ServerFnError>> + 'static,
    {
        let pending = RwSignal::new(false);
        let message = config.message.unwrap_or_else(|| RwSignal::new(None));
        let tracker = StoredValue::new(Tracker::default());
        let run = Arc::new(run);
        let on_ok = Arc::new(on_ok);
        let start: Arc<dyn Fn(I) + Send + Sync> = Arc::new(move |input: I| {
            let mut begun = false;
            tracker.update_value(|t| begun = t.begin());
            if !begun {
                return;
            }
            pending.set(true);
            message.set(None);
            let (run, on_ok) = (run.clone(), on_ok.clone());
            spawn_local(async move {
                let transport = run(input).await.map_err(|_| ());
                let mut done = None;
                tracker.update_value(|t| {
                    done = Some(t.finish(transport, config.fallback, config.conflict));
                });
                pending.set(false);
                match done {
                    Some(Finished::Success(value)) => on_ok(value),
                    Some(Finished::Failure(text)) => message.set(Some(text)),
                    None => {}
                }
            });
        });
        Self {
            pending,
            message,
            start: StoredValue::new(start),
        }
    }

    /// Запускает команду.
    pub fn run(&self, input: I) {
        (self.start.get_value())(input);
    }

    /// Идёт ли команда сейчас.
    pub fn pending(&self) -> Signal<bool> {
        self.pending.into()
    }
}
