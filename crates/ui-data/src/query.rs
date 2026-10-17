//! Чтение данных: ресурс с перезагрузкой и компонент, который показывает загрузку, ошибку,
//! пустой результат либо данные.
use std::future::Future;

use leptos::prelude::*;
use serde::{Serialize, de::DeserializeOwned};
use ui::{EmptyState, Notice, NoticeKind};
use ui_core::Reply;

/// Ресурс запроса и счётчик перезагрузки.
pub struct QueryHandle<T: Send + Sync + Serialize + DeserializeOwned + 'static> {
    resource: Resource<Result<Reply<T>, ServerFnError>>,
    version: RwSignal<u32>,
}

impl<T: Send + Sync + Serialize + DeserializeOwned + 'static> Clone for QueryHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + Serialize + DeserializeOwned + 'static> Copy for QueryHandle<T> {}

impl<T: Send + Sync + Serialize + DeserializeOwned + 'static> QueryHandle<T> {
    /// Перечитывает данные; прежние остаются на экране до ответа.
    pub fn reload(&self) {
        self.version.update(|v| *v += 1);
    }
}

/// Создаёт запрос: `key` задаёт параметры (при их изменении данные читаются заново), `fetch` -
/// серверная функция.
pub fn use_query<K, T, Fut>(
    key: impl Fn() -> K + Send + Sync + 'static,
    fetch: impl Fn(K) -> Fut + Send + Sync + 'static,
) -> QueryHandle<T>
where
    K: Clone + PartialEq + Send + Sync + 'static,
    T: Send + Sync + Serialize + DeserializeOwned + 'static,
    Fut: Future<Output = Result<Reply<T>, ServerFnError>> + Send + 'static,
{
    let version = RwSignal::new(0u32);
    let resource = Resource::new(move || (key(), version.get()), move |(k, _)| fetch(k));
    QueryHandle { resource, version }
}

/// Показывает результат запроса: «Загрузка…», сообщение об ошибке, пустое состояние или данные.
#[component]
pub fn Query<T>(
    handle: QueryHandle<T>,
    /// Представление данных.
    view: Callback<T, AnyView>,
    /// Когда результат считается пустым; без неё пустого состояния нет.
    #[prop(optional)]
    is_empty: Option<fn(&T) -> bool>,
    #[prop(default = "Пока ничего нет")] empty_title: &'static str,
    #[prop(default = "Записи появятся здесь.")] empty_hint: &'static str,
    #[prop(default = "Не удалось загрузить данные. Попробуйте позже.")] error: &'static str,
) -> impl IntoView
where
    T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
    view! {
        <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
            {move || {
                handle
                    .resource
                    .get()
                    .map(|result| match result {
                        Ok(Reply::Ok(data)) => {
                            if is_empty.is_some_and(|f| f(&data)) {
                                view! { <EmptyState title=empty_title hint=empty_hint /> }.into_any()
                            } else {
                                view.run(data)
                            }
                        }
                        Ok(other) => {
                            let text = other.failure().unwrap_or_else(|| error.to_string());
                            view! { <Notice kind=NoticeKind::Error>{text}</Notice> }.into_any()
                        }
                        Err(_) => view! { <Notice kind=NoticeKind::Error>{error}</Notice> }.into_any(),
                    })
            }}
        </Transition>
    }
}
