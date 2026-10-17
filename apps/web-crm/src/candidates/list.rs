//! Список кандидатов на гриде: поиск, сортировка, фильтры столбцов и по заказчику.
use candidate_contracts::PersonSummaryDto;
use grid_contracts::Operator;
use leptos::prelude::*;
use leptos_router::components::A;
use ui_data::EntityGrid;
use ui_kit::{PageHeader, Reply, SessionCtx, grid::Column};

use super::{
    format::{format_salary, years_label},
    server::list_persons_grid,
};
use crate::clients::server::list_clients;

/// Задержка перед запросом после ввода, мс.
const DEBOUNCE_MS: u64 = 250;

/// Может ли пользователь создавать и менять данные (по сессии).
pub fn can_write(ctx: SessionCtx) -> bool {
    session_user(ctx).is_some_and(|u| u.policy().can_write())
}

/// Может ли пользователь удалять данные организации (владелец или роль с правом удаления).
pub fn can_delete(ctx: SessionCtx) -> bool {
    session_user(ctx).is_some_and(|u| u.policy().can_delete())
}

/// Владелец организации: ему доступен раздел «Безопасность».
pub fn is_owner(ctx: SessionCtx) -> bool {
    session_user(ctx).is_some_and(|u| u.policy().is_owner())
}

fn session_user(ctx: SessionCtx) -> Option<ui_kit::UserInfo> {
    ctx.current().and_then(Result::ok).and_then(|s| s.user)
}

/// Поле, значение которого применяется после паузы в наборе.
#[component]
pub fn DebouncedInput(
    applied: WriteSignal<String>,
    reset_page: WriteSignal<i64>,
    #[prop(into)] placeholder: String,
    #[prop(into)] label: String,
    testid: &'static str,
) -> impl IntoView {
    let generation = RwSignal::new(0u32);
    view! {
        <input
            type="search"
            placeholder=placeholder
            aria-label=label
            data-testid=testid
            on:input=move |ev| {
                let value = event_target_value(&ev);
                let current = generation.get_untracked() + 1;
                generation.set(current);
                set_timeout(
                    move || {
                        if generation.get_untracked() == current {
                            applied.set(value);
                            reset_page.set(1);
                        }
                    },
                    std::time::Duration::from_millis(DEBOUNCE_MS),
                );
            }
        />
    }
}

fn person_href(person: &PersonSummaryDto) -> String {
    format!("/candidates/{}", person.id)
}

fn person_columns(clients: Signal<Vec<(String, String)>>) -> Vec<Column<PersonSummaryDto>> {
    vec![
        Column::new("full_name", "Кандидат", |p: &PersonSummaryDto| {
            view! { <span data-testid="person-name">{p.full_name.clone()}</span> }.into_any()
        })
        .sortable()
        .filter_text(),
        Column::new("email", "Контакты", |p: &PersonSummaryDto| {
            let contacts = [p.phone.clone(), p.email.clone()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
            view! { <span>{contacts}</span> }.into_any()
        })
        .sortable()
        .filter_text(),
        Column::new("source", "Источник", |p: &PersonSummaryDto| {
            view! { <span>{p.source.clone().unwrap_or_default()}</span> }.into_any()
        })
        .sortable()
        .filter_text(),
        Column::new("skills", "Навыки", |p: &PersonSummaryDto| {
            view! { <span>{p.skills.join(", ")}</span> }.into_any()
        })
        .filter_text(),
        Column::new("experience_years", "Стаж", |p: &PersonSummaryDto| {
            view! { <span>{years_label(p.experience_years)}</span> }.into_any()
        })
        .sortable()
        .filter_text_op(Operator::Gte),
        Column::new(
            "expected_salary",
            "Зарплата",
            |p: &PersonSummaryDto| {
                view! { <span>{p.expected_salary.map(format_salary).unwrap_or_default()}</span> }
                    .into_any()
            },
        )
        .sortable(),
        // кандидаты заказчика: люди с откликами на его вакансии
        Column::filter_only("client", "Заказчик")
            .filter_select(clients, Operator::Eq)
            .empty_label("Все заказчики"),
    ]
}

#[component]
pub fn CandidatesPage() -> impl IntoView {
    let ctx = expect_context::<SessionCtx>();
    let truncated = RwSignal::new(false);
    let clients = LocalResource::new(|| list_clients(String::new(), true));
    let client_options = Signal::derive(move || {
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
    });
    let load = move |query| async move {
        let (reply, cut) = list_persons_grid(query).await?;
        truncated.set(cut);
        Ok::<_, ServerFnError>(reply)
    };

    view! {
        <PageHeader title="Кандидаты">
            <Show when=move || can_write(ctx)>
                <A href="/candidates/import" attr:class="btn" attr:data-testid="import-button">
                    "Импорт"
                </A>
                <A href="/candidates/new" attr:class="btn btn--primary">
                    "Добавить кандидата"
                </A>
            </Show>
        </PageHeader>

        {move || {
            truncated
                .get()
                .then(|| {
                    view! {
                        <p class="muted" data-testid="client-people-truncated">
                            "У заказчика больше 200 откликнувшихся: показаны первые 200, остальных ищите на экране «Отклики»."
                        </p>
                    }
                })
        }}

        <EntityGrid
            columns=person_columns(client_options)
            load=load
            row_href=Some(person_href as fn(&PersonSummaryDto) -> String)
            search_fields=vec!["full_name", "email", "phone", "city", "skills"]
            search_placeholder="Поиск по имени, email, телефону, навыкам"
            empty_title="Кандидатов пока нет"
            empty_hint="Добавьте первого кандидата, и он появится здесь."
        />
    }
}
