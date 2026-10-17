//! Сетка записей: `Grid` над серверной функцией, которая отвечает `Reply`, и удаление записи с подтверждением.
use std::future::Future;

use grid_contracts::{GridPage, GridQuery};
use leptos::prelude::*;
use serde::{Serialize, de::DeserializeOwned};
use ui::{Column, Grid, RowRemove};
use ui_core::Reply;

/// Удаление записи из строки: название для вопроса, правило «можно ли» и серверная функция.
pub struct EntityRemove<T> {
    inner: RowRemove<T>,
}

impl<T: Send + Sync + 'static> EntityRemove<T> {
    pub fn new<F, Fut>(label: fn(&T) -> String, delete: F) -> Self
    where
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Reply<()>, ServerFnError>> + Send + 'static,
    {
        let inner = RowRemove::new(label, move |item| {
            let call = delete(item);
            async move {
                match call.await {
                    Ok(reply) => reply.failure().map_or(Ok(()), Err),
                    Err(_) => Err("Не удалось удалить запись. Попробуйте позже.".to_string()),
                }
            }
        });
        Self { inner }
    }

    /// Какие записи можно удалять.
    pub fn allowed_when(mut self, allowed: fn(&T) -> bool) -> Self {
        self.inner = self.inner.allowed_when(allowed);
        self
    }
}

/// Преобразует ответ серверной функции в то, что ждёт `Grid`.
fn page_or_text<T>(
    result: Result<Reply<GridPage<T>>, ServerFnError>,
) -> Result<GridPage<T>, String> {
    match result {
        Ok(Reply::Ok(page)) => Ok(page),
        Ok(other) => Err(other
            .failure()
            .unwrap_or_else(|| "Не удалось загрузить список.".into())),
        Err(_) => Err("Не удалось загрузить список. Попробуйте позже.".into()),
    }
}

/// Список записей. `load` - серверная функция списка, ответ которой `Reply`; остальное - как у `Grid`.
#[component]
pub fn EntityGrid<T, F, Fut>(
    columns: Vec<Column<T>>,
    load: F,
    #[prop(default = None)] row_href: Option<fn(&T) -> String>,
    #[prop(default = None)] remove: Option<EntityRemove<T>>,
    #[prop(optional)] search_fields: Vec<&'static str>,
    #[prop(default = "Поиск")] search_placeholder: &'static str,
    #[prop(default = "Пока ничего нет")] empty_title: &'static str,
    #[prop(default = "Записи появятся здесь.")] empty_hint: &'static str,
    #[prop(default = None)] toolbar: Option<ViewFn>,
) -> impl IntoView
where
    T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
    F: Fn(GridQuery) -> Fut + Copy + Send + Sync + 'static,
    Fut: Future<Output = Result<Reply<GridPage<T>>, ServerFnError>> + Send + 'static,
{
    let adapted = move |query: GridQuery| {
        let call = load(query);
        async move { page_or_text(call.await) }
    };
    view! {
        <Grid
            columns=columns
            load=adapted
            row_href=row_href
            remove=remove.map(|r| r.inner)
            search_fields=search_fields
            search_placeholder=search_placeholder
            empty_title=empty_title
            empty_hint=empty_hint
            toolbar=toolbar
        />
    }
}
