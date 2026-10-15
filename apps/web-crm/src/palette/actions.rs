//! Кнопки быстрых действий в шапке: «+ Добавить», «Разбор», «?».
use leptos::prelude::*;
use leptos_router::components::A;

use super::view::PaletteCtx;

#[component]
pub fn ShellActions(palette: PaletteCtx, writable: Signal<bool>) -> impl IntoView {
    let menu = palette.menu;
    // клик вне меню закрывает его (Esc обрабатывает общий обработчик клавиатуры)
    Effect::new(move |_| {
        window_event_listener(leptos::ev::click, move |ev| {
            use leptos::wasm_bindgen::JsCast;
            if !menu.get_untracked() {
                return;
            }
            let inside = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                .is_some_and(|el| el.closest(".topmenu").ok().flatten().is_some());
            if !inside {
                menu.set(false);
            }
        });
    });

    view! {
        <Show when=move || writable.get()>
            <div class="topmenu">
                <button
                    type="button"
                    class="topbtn topbtn--pri"
                    data-testid="add-button"
                    aria-haspopup="menu"
                    aria-expanded=move || menu.get().to_string()
                    on:click=move |_| menu.update(|m| *m = !*m)
                >
                    <span aria-hidden="true">"+"</span>
                    <span class="topbtn__text">" Добавить"</span>
                </button>
                <Show when=move || menu.get()>
                    <div class="topmenu__list" role="menu" data-testid="add-menu">
                        <a role="menuitem" href="/candidates/new" data-testid="add-candidate" on:click=move |_| menu.set(false)>
                            <span class="t">"Кандидат"</span>
                            <span class="s">"карточка вручную"</span>
                        </a>
                        <a role="menuitem" href="/vacancies?new=1" data-testid="add-vacancy" on:click=move |_| menu.set(false)>
                            <span class="t">"Вакансия"</span>
                            <span class="s">"и сразу её воронка"</span>
                        </a>
                        <a role="menuitem" href="/candidates/import" data-testid="add-import" on:click=move |_| menu.set(false)>
                            <span class="t">"Импорт из файла"</span>
                            <kbd>"I"</kbd>
                        </a>
                    </div>
                </Show>
            </div>
            <A href="/triage" attr:class="topbtn" attr:data-testid="shell-triage">
                <span class="dot dot--hot" aria-hidden="true"></span>
                <span class="topbtn__text">"Разбор"</span>
                <kbd>"T"</kbd>
            </A>
        </Show>
        <button
            type="button"
            class="topbtn"
            aria-label="Горячие клавиши"
            data-testid="shell-cheat"
            on:click=move |_| palette.cheat.set(true)
        >
            <kbd>"?"</kbd>
        </button>
    }
}
