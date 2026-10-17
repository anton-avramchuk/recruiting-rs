//! Интерфейс палитры команд и шпаргалки, обработчик клавиатуры оболочки.
use std::time::Duration;

use leptos::{prelude::*, task::spawn_local};
use leptos_router::{NavigateOptions, hooks::use_navigate};

use super::{
    commands::{
        Entry, Hit, MAX_CLIENTS, MAX_PEOPLE, Target, match_vacancies, results, with_owner_commands,
    },
    keys::{CHEATSHEET, Press, Sequence, Shortcut},
};
use ui_kit::SessionCtx;

use crate::{
    candidates::{
        list::is_owner,
        outcome::ListOutcome,
        server::{PersonQuery, search_persons},
    },
    funnel::{format::now_ms, server::list_vacancies},
};

/// Задержка ввода перед обращением к серверу, мс.
const SEARCH_DEBOUNCE_MS: u64 = 200;

/// Открытые окна оболочки.
#[derive(Clone, Copy)]
pub struct PaletteCtx {
    pub open: RwSignal<bool>,
    pub cheat: RwSignal<bool>,
    /// Меню «+ Добавить» в шапке.
    pub menu: RwSignal<bool>,
}

impl PaletteCtx {
    pub fn new() -> Self {
        Self {
            open: RwSignal::new(false),
            cheat: RwSignal::new(false),
            menu: RwSignal::new(false),
        }
    }
}

impl Default for PaletteCtx {
    fn default() -> Self {
        Self::new()
    }
}

/// Фокус в поле ввода: горячие клавиши не должны перехватывать набор текста.
pub(crate) fn typing_in_a_field(ev: &leptos::ev::KeyboardEvent) -> bool {
    use leptos::wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
        .is_some_and(|el| {
            matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT")
                || el.is_content_editable()
        })
}

fn panel_is_open() -> bool {
    document()
        .query_selector("[data-testid=panel]")
        .ok()
        .flatten()
        .is_some()
}

/// Esc в поле ввода снимает фокус; возвращает, было ли поле.
pub(crate) fn blur_active_field(ev: &leptos::ev::KeyboardEvent) -> bool {
    use leptos::wasm_bindgen::JsCast;
    if !typing_in_a_field(ev) {
        return false;
    }
    if let Some(el) = ev
        .target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = el.blur();
    }
    true
}

/// Нажатие в виде, удобном для разбора, без хранения строк в обработчиках.
pub(crate) fn with_press<R>(ev: &leptos::ev::KeyboardEvent, f: impl FnOnce(Press<'_>) -> R) -> R {
    let (key, code) = (ev.key(), ev.code());
    f(Press {
        key: &key,
        code: &code,
        shift: ev.shift_key(),
    })
}

/// Открыты ли палитра или шпаргалка (страничные клавиши в это время молчат).
/// Контекст берётся при вызове в теле компонента: из обработчика события его уже не достать.
pub(crate) fn overlay_check() -> impl Fn() -> bool + Copy + 'static {
    let palette = use_context::<PaletteCtx>();
    move || palette.is_some_and(|p| p.open.get_untracked() || p.cheat.get_untracked())
}

/// Фокусирует элемент, как только он появится (после перехода на другой экран).
pub(crate) fn focus_when_ready(selector: &'static str, tries: u32) {
    use leptos::wasm_bindgen::JsCast;
    let found = document()
        .query_selector(selector)
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok());
    match found {
        Some(el) => {
            let _ = el.focus();
        }
        None if tries > 0 => set_timeout(
            move || focus_when_ready(selector, tries - 1),
            Duration::from_millis(50),
        ),
        None => {}
    }
}

/// Единый обработчик клавиатуры защищённой части: Ctrl/⌘ K, `G + буква`, `/`, `I`, `T`, `?`, Esc.
pub fn install_keys(palette: PaletteCtx, writable: Signal<bool>) {
    // на сервере локальные значения могут умереть в другом потоке: навигацию держим обычным значением
    let navigate = use_navigate();
    let sequence = StoredValue::new(Sequence::default());
    Effect::new(move |_| {
        let navigate = navigate.clone();
        window_event_listener(leptos::ev::keydown, move |ev| {
            if (ev.ctrl_key() || ev.meta_key()) && ev.code() == "KeyK" {
                ev.prevent_default();
                sequence.update_value(Sequence::reset);
                palette.cheat.set(false);
                palette.menu.set(false);
                palette.open.update(|o| *o = !*o);
                return;
            }
            if ev.ctrl_key() || ev.meta_key() || ev.alt_key() {
                return;
            }
            if ev.key() == "Escape" {
                // отмеченное событие не должно закрывать что-то ещё (панель, выделение)
                if palette.cheat.get_untracked() {
                    ev.prevent_default();
                    palette.cheat.set(false);
                } else if palette.open.get_untracked() {
                    ev.prevent_default();
                    palette.open.set(false);
                } else if palette.menu.get_untracked() {
                    ev.prevent_default();
                    palette.menu.set(false);
                } else if blur_active_field(&ev) {
                    ev.prevent_default();
                }
                return;
            }
            if palette.open.get_untracked() || palette.cheat.get_untracked() {
                return;
            }
            if typing_in_a_field(&ev) || panel_is_open() {
                sequence.update_value(Sequence::reset);
                return;
            }
            let key = ev.key();
            let code = ev.code();
            let press = Press {
                key: &key,
                code: &code,
                shift: ev.shift_key(),
            };
            let mut found = None;
            sequence.update_value(|s| found = s.press(press, now_ms()));
            let Some(shortcut) = found else { return };
            if shortcut.needs_write() && !writable.get_untracked() {
                return;
            }
            ev.prevent_default();
            let go = |path: &str| navigate(path, NavigateOptions::default());
            match shortcut {
                Shortcut::Go(path) => go(path),
                Shortcut::Search => {
                    go("/candidates");
                    focus_when_ready("[data-testid=search]", 10);
                }
                Shortcut::Import => go("/candidates/import"),
                Shortcut::Triage => go("/triage"),
                Shortcut::Cheatsheet => palette.cheat.set(true),
            }
        });
    });
}

/// Окна палитры и шпаргалки.
#[component]
pub fn Overlays(palette: PaletteCtx, writable: Signal<bool>) -> impl IntoView {
    view! {
        {move || palette.open.get().then(|| view! { <PaletteDialog palette writable /> })}
        {move || palette.cheat.get().then(|| view! { <Cheatsheet palette /> })}
    }
}

fn person_hit(p: candidate_contracts::PersonSummaryDto) -> Hit {
    let meta = [p.city, p.source]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    Hit {
        id: p.id.to_string(),
        title: p.full_name,
        meta,
    }
}

/// Заказчики по запросу; при ошибке сервиса — пусто, команды остаются доступны.
async fn find_clients(q: String) -> Vec<Hit> {
    if q.trim().is_empty() {
        return Vec::new();
    }
    match crate::clients::server::list_clients(q, false).await {
        Ok(ui_kit::Reply::Ok(list)) => list
            .into_iter()
            .take(MAX_CLIENTS)
            .map(|c| Hit {
                id: c.id.to_string(),
                title: c.name,
                meta: match c.contacts_count {
                    0 => String::new(),
                    n => format!("{n} конт."),
                },
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Люди по запросу; при ошибке сервиса — пусто, команды остаются доступны.
async fn find_people(q: String) -> Vec<Hit> {
    if q.trim().is_empty() {
        return Vec::new();
    }
    let query = PersonQuery {
        q,
        page: 1,
        ..Default::default()
    };
    match search_persons(query).await {
        Ok(ListOutcome::Page(page)) => page
            .items
            .into_iter()
            .take(MAX_PEOPLE)
            .map(person_hit)
            .collect(),
        _ => Vec::new(),
    }
}

#[component]
fn PaletteDialog(palette: PaletteCtx, writable: Signal<bool>) -> impl IntoView {
    use leptos::wasm_bindgen::JsCast;

    let query = RwSignal::new(String::new());
    let selected = RwSignal::new(0usize);
    let all_vacancies = RwSignal::new(Vec::<Hit>::new());
    let people = RwSignal::new(Vec::<Hit>::new());
    let found_clients = RwSignal::new(Vec::<Hit>::new());
    let generation = RwSignal::new(0u32);
    let input = NodeRef::<leptos::html::Input>::new();
    let nav = StoredValue::new_local(use_navigate());

    // куда вернуть фокус после закрытия
    let previous = StoredValue::new_local(
        document()
            .active_element()
            .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok()),
    );
    on_cleanup(move || {
        if let Some(Some(el)) = previous.try_get_value() {
            let _ = el.focus();
        }
    });
    input.on_load(|el| {
        let _ = el.focus();
    });

    // список вакансий небольшой: грузим один раз и фильтруем на месте
    Effect::new(move |_| {
        spawn_local(async move {
            if let Some(list) = list_vacancies().await.ok().and_then(|r| r.ok()) {
                let hits = list
                    .into_iter()
                    .map(|v| Hit {
                        id: v.id.to_string(),
                        meta: [v.client.map(|c| c.name), v.city]
                            .into_iter()
                            .flatten()
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join(" · "),
                        title: v.title,
                    })
                    .collect();
                all_vacancies.try_set(hits);
            }
        });
    });

    let vacancies = Memo::new(move |_| match_vacancies(&query.get(), &all_vacancies.get()));
    let session = expect_context::<SessionCtx>();
    let entries = Memo::new(move |_| {
        let rows = results(
            &query.get(),
            writable.get(),
            &found_clients.get(),
            &vacancies.get(),
            &people.get(),
        );
        with_owner_commands(rows, &query.get(), is_owner(session))
    });

    let run = move |entry: Entry| {
        palette.open.set(false);
        match entry.target {
            Target::Go(path) => nav.with_value(|n| n(&path, NavigateOptions::default())),
            Target::Cheatsheet => palette.cheat.set(true),
            Target::ToggleTheme => {
                ui_kit::theme::toggle_theme();
            }
        }
    };

    let on_input = move |ev| {
        let value = event_target_value(&ev);
        selected.set(0);
        if value.trim().is_empty() {
            people.set(Vec::new());
            found_clients.set(Vec::new());
        }
        query.set(value.clone());
        let current = generation.get_untracked() + 1;
        generation.set(current);
        set_timeout(
            move || {
                if generation.try_get_untracked() != Some(current) || value.trim().is_empty() {
                    return;
                }
                spawn_local(async move {
                    // заказчики и люди ищутся независимо: сбой одного поиска не прячет другой
                    let (clients_hits, people_hits) =
                        (find_clients(value.clone()).await, find_people(value).await);
                    // ответ на устаревший запрос отбрасывается
                    if generation.try_get_untracked() == Some(current) {
                        found_clients.try_set(clients_hits);
                        people.try_set(people_hits);
                    }
                });
            },
            Duration::from_millis(SEARCH_DEBOUNCE_MS),
        );
    };

    let on_keydown = move |ev: leptos::ev::KeyboardEvent| match ev.key().as_str() {
        "ArrowDown" => {
            ev.prevent_default();
            let last = entries.get_untracked().len().saturating_sub(1);
            selected.update(|s| *s = (*s + 1).min(last));
        }
        "ArrowUp" => {
            ev.prevent_default();
            selected.update(|s| *s = s.saturating_sub(1));
        }
        "Enter" => {
            ev.prevent_default();
            if let Some(entry) = entries.get_untracked().get(selected.get_untracked()) {
                run(entry.clone());
            }
        }
        _ => {}
    };

    // выбранная строка остаётся в видимой части списка
    Effect::new(move |_| {
        selected.track();
        set_timeout(
            || {
                if let Ok(Some(el)) = document().query_selector(".pitem.on") {
                    el.scroll_into_view();
                }
            },
            Duration::from_millis(0),
        );
    });

    let rows = move || {
        let list = entries.get();
        let current = selected.get();
        if list.is_empty() {
            return view! { <div class="palette__empty" data-testid="palette-empty">"Ничего не нашлось"</div> }
                .into_any();
        }
        let mut out = Vec::new();
        let mut group = None;
        for (i, entry) in list.into_iter().enumerate() {
            if group != Some(entry.group) {
                group = Some(entry.group);
                out.push(
                    view! { <div class="pgrp" role="presentation">{entry.group.label()}</div> }
                        .into_any(),
                );
            }
            let title = entry.title.clone();
            let hint = entry.hint.clone();
            out.push(
                view! {
                    <button
                        type="button"
                        class="pitem"
                        class:on=(i == current)
                        role="option"
                        aria-selected=(i == current).to_string()
                        data-testid="palette-item"
                        on:mousemove=move |_| {
                            if selected.get_untracked() != i {
                                selected.set(i);
                            }
                        }
                        on:click=move |_| run(entry.clone())
                    >
                        <span class="t">{title}</span>
                        <span class="s">{hint}</span>
                    </button>
                }
                .into_any(),
            );
        }
        out.into_any()
    };

    view! {
        <div class="scrim" data-testid="palette-scrim" on:click=move |_| palette.open.set(false)></div>
        <div
            class="palette"
            role="dialog"
            aria-modal="true"
            aria-label="Палитра команд"
            data-testid="palette"
            on:keydown=on_keydown
        >
            <input
                node_ref=input
                class="palette__input"
                type="text"
                autocomplete="off"
                placeholder="Что сделать или кого найти…"
                aria-label="Поиск по командам, вакансиям и людям"
                data-testid="palette-input"
                on:input=on_input
            />
            <div class="palette__list" role="listbox">{rows}</div>
            <div class="palette__foot">
                <span><kbd>"↑"</kbd><kbd>"↓"</kbd>" выбрать"</span>
                <span><kbd>"Enter"</kbd>" открыть"</span>
                <span><kbd>"Esc"</kbd>" закрыть"</span>
            </div>
        </div>
    }
}

#[component]
fn Cheatsheet(palette: PaletteCtx) -> impl IntoView {
    view! {
        <div class="scrim" data-testid="cheat-scrim" on:click=move |_| palette.cheat.set(false)></div>
        <div class="palette cheat" role="dialog" aria-modal="true" aria-label="Горячие клавиши" data-testid="cheatsheet">
            <div class="sec-h">
                <h2>"Горячие клавиши"</h2>
                <span class="spacer"></span>
                <button type="button" class="btn" data-testid="cheat-close" on:click=move |_| palette.cheat.set(false)>
                    <kbd>"Esc"</kbd>
                </button>
            </div>
            <p class="muted">"Клавиши не срабатывают, пока вы печатаете в поле."</p>
            <div class="cheatgrid">
                {CHEATSHEET
                    .iter()
                    .map(|(keys, text)| {
                        view! {
                            <div class="crow" data-testid="cheat-row">
                                <span class="k">
                                    {keys.iter().map(|k| view! { <kbd>{*k}</kbd> }).collect_view()}
                                </span>
                                <span class="d">{*text}</span>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}
