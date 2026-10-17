//! Варианты выбора из справочников организации для `Dropdown`: заказчики и вакансии.
//! Данные читаются в браузере; пока они не пришли, список пуст.
use leptos::prelude::*;
use ui_kit::Reply;

use crate::{clients::server::list_clients, funnel::server::list_vacancies};

/// Заказчики организации. Архивные включаются, если `with_archived`, и помечаются «(архив)».
pub fn client_options(with_archived: bool) -> Signal<Vec<(String, String)>> {
    let clients = LocalResource::new(move || list_clients(String::new(), with_archived));
    Signal::derive(move || {
        clients
            .get()
            .and_then(Result::ok)
            .and_then(Reply::ok)
            .map(|list| {
                list.into_iter()
                    .map(|c| {
                        let label = if c.archived {
                            format!("{} (архив)", c.name)
                        } else {
                            c.name
                        };
                        (c.id.to_string(), label)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    })
}

/// Вакансии организации.
pub fn vacancy_options() -> Signal<Vec<(String, String)>> {
    let vacancies = LocalResource::new(list_vacancies);
    Signal::derive(move || {
        vacancies
            .get()
            .and_then(Result::ok)
            .and_then(Reply::ok)
            .map(|list| {
                list.into_iter()
                    .map(|v| (v.id.to_string(), v.title))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    })
}
