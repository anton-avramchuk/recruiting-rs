//! Список кандидатов: поиск, фильтры, постраничная выдача.
use leptos::prelude::*;
use leptos_router::components::A;
use ui_kit::Reply;
use ui_kit::{EmptyState, Notice, NoticeKind, PageHeader, SessionCtx};

use super::{
    format::{format_salary, years_label},
    outcome::ListOutcome,
    server::{PersonQuery, search_persons_of_client},
};
use crate::clients::server::list_clients;

/// Задержка перед запросом после ввода, мс.
const DEBOUNCE_MS: u64 = 250;

/// Может ли пользователь создавать и менять данные (по сессии).
pub fn can_write(ctx: SessionCtx) -> bool {
    session_user(ctx).is_some_and(|u| crate::access::can_write(&u))
}

/// Может ли пользователь удалять данные организации (владелец или роль с правом удаления).
pub fn can_delete(ctx: SessionCtx) -> bool {
    session_user(ctx).is_some_and(|u| crate::access::can_delete(&u))
}

/// Владелец организации: ему доступен раздел «Безопасность».
pub fn is_owner(ctx: SessionCtx) -> bool {
    session_user(ctx).is_some_and(|u| crate::access::is_owner(&u))
}

fn session_user(ctx: SessionCtx) -> Option<ui_kit::UserInfo> {
    ctx.current().and_then(Result::ok).and_then(|s| s.user)
}

/// Есть ли непустые текстовые фильтры (поиск, источник, навык, стаж).
fn has_text_filters(
    q: ReadSignal<String>,
    source: ReadSignal<String>,
    skill: ReadSignal<String>,
    min_experience: ReadSignal<String>,
) -> bool {
    !q.get_untracked().trim().is_empty()
        || !source.get_untracked().trim().is_empty()
        || !skill.get_untracked().trim().is_empty()
        || !min_experience.get_untracked().trim().is_empty()
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

#[component]
pub fn CandidatesPage() -> impl IntoView {
    let ctx = expect_context::<SessionCtx>();
    let (q, set_q) = signal(String::new());
    let (source, set_source) = signal(String::new());
    let (skill, set_skill) = signal(String::new());
    let (min_experience, set_min_experience) = signal(String::new());
    let (page, set_page) = signal(1i64);
    // выбранный заказчик: пустая строка - все; epoch пересоздаёт поля ввода при сбросе
    let client = RwSignal::new(String::new());
    let epoch = RwSignal::new(0u32);
    let truncated = RwSignal::new(false);
    let clients = Resource::new(|| (), |_| list_clients(String::new(), true));

    let results = Resource::new(
        move || {
            (
                q.get(),
                source.get(),
                skill.get(),
                min_experience.get(),
                page.get(),
                client.get(),
            )
        },
        move |(q, source, skill, min_experience, page, client)| async move {
            let query = PersonQuery {
                q,
                source,
                skill,
                min_experience,
                ids: String::new(),
                page,
            };
            let (outcome, cut) = search_persons_of_client(query, client).await?;
            truncated.set(cut);
            Ok::<_, ServerFnError>(outcome)
        },
    );
    let filtered = move || {
        !q.get().trim().is_empty()
            || !source.get().trim().is_empty()
            || !skill.get().trim().is_empty()
            || !min_experience.get().trim().is_empty()
            || !client.get().is_empty()
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
            epoch.get();
            view! {
                <div class="toolbar">
                    <DebouncedInput
                applied=set_q
                reset_page=set_page
                placeholder="Поиск по имени, email, телефону, навыкам"
                label="Поиск"
                testid="search"
            />
            <DebouncedInput
                applied=set_source
                reset_page=set_page
                placeholder="Источник"
                label="Источник"
                testid="filter-source"
            />
            <DebouncedInput
                applied=set_skill
                reset_page=set_page
                placeholder="Навык"
                label="Навык"
                testid="filter-skill"
            />
            <DebouncedInput
                applied=set_min_experience
                reset_page=set_page
                placeholder="Стаж от, лет"
                label="Стаж от"
                testid="filter-min-experience"
            />
            <select
                aria-label="Заказчик"
                data-testid="filter-client"
                prop:value=move || client.get()
                on:change=move |ev| {
                    client.set(event_target_value(&ev));
                    set_page.set(1);
                }
            >
                <option value="">"Все заказчики"</option>
                <Transition fallback=|| ()>
                    {move || {
                        clients
                            .get()
                            .and_then(Result::ok)
                            .and_then(Reply::ok)
                            .map(|list| {
                                list.into_iter()
                                    .map(|c| {
                                        let label = if c.archived { format!("{} (архив)", c.name) } else { c.name.clone() };
                                        let id = c.id.to_string();
                                        view! { <option value=id.clone() selected=move || client.get() == id>{label}</option> }
                                    })
                                    .collect_view()
                            })
                    }}
                </Transition>
            </select>
            <Show when=filtered>
                <button
                    class="btn btn--ghost"
                    data-testid="reset-filters"
                    on:click=move |_| {
                        set_q.set(String::new());
                        set_source.set(String::new());
                        set_skill.set(String::new());
                        set_min_experience.set(String::new());
                        client.set(String::new());
                        set_page.set(1);
                        epoch.update(|e| *e += 1);
                    }
                >
                    "Сбросить фильтры"
                </button>
            </Show>
        </div>
            }
        }}
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

        <Transition fallback=|| view! { <p class="muted">"Загрузка…"</p> }>
            {move || {
                results
                    .get()
                    .map(|result| match result {
                        Ok(ListOutcome::Page(data)) if data.items.is_empty() => {
                            if !client.get_untracked().is_empty() && !has_text_filters(q, source, skill, min_experience) {
                                view! {
                                    <EmptyState
                                        title="Кандидатов заказчика пока нет"
                                        hint="На его вакансии ещё никто не откликался."
                                    />
                                }
                                    .into_any()
                            } else if filtered() {
                                view! {
                                    <EmptyState
                                        title="Ничего не найдено"
                                        hint="Измените запрос или сбросьте фильтры."
                                    />
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <EmptyState
                                        title="Кандидатов пока нет"
                                        hint="Добавьте первого кандидата, и он появится здесь."
                                    />
                                }
                                    .into_any()
                            }
                        }
                        Ok(ListOutcome::Page(data)) => {
                            let pages = ((data.total + data.page_size - 1) / data.page_size).max(1);
                            let at_end = move || page.get() >= pages;
                            view! {
                                <p class="muted">{format!("Найдено: {}", data.total)}</p>
                                <div class="table-wrap">
                                    <table data-testid="persons">
                                        <thead>
                                            <tr>
                                                <th>"Кандидат"</th>
                                                <th>"Контакты"</th>
                                                <th>"Навыки"</th>
                                                <th>"Стаж"</th>
                                                <th>"Зарплата"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {data
                                                .items
                                                .into_iter()
                                                .map(|p| {
                                                    let contacts = [p.phone.clone(), p.email.clone()]
                                                        .into_iter()
                                                        .flatten()
                                                        .collect::<Vec<_>>()
                                                        .join(" · ");
                                                    view! {
                                                        <tr>
                                                            <td>
                                                                <A href=format!("/candidates/{}", p.id)>
                                                                    {p.full_name.clone()}
                                                                </A>
                                                            </td>
                                                            <td>{contacts}</td>
                                                            <td>{p.skills.join(", ")}</td>
                                                            <td>{years_label(p.experience_years)}</td>
                                                            <td>
                                                                {p.expected_salary.map(format_salary).unwrap_or_default()}
                                                            </td>
                                                        </tr>
                                                    }
                                                })
                                                .collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                                <div class="toolbar">
                                    <span class="muted">{format!("Страница {} из {}", data.page, pages)}</span>
                                    <button
                                        class="btn"
                                        disabled=move || page.get() <= 1
                                        on:click=move |_| set_page.update(|p| *p -= 1)
                                    >
                                        "Назад"
                                    </button>
                                    <button class="btn" disabled=at_end on:click=move |_| set_page.update(|p| *p += 1)>
                                        "Вперёд"
                                    </button>
                                </div>
                            }
                                .into_any()
                        }
                        _ => {
                            view! {
                                <Notice kind=NoticeKind::Error>
                                    "Не удалось загрузить кандидатов. Попробуйте позже."
                                </Notice>
                            }
                                .into_any()
                        }
                    })
            }}
        </Transition>
    }
}
