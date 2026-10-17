//! Пейджер: подпись, выбор размера страницы, «Назад» и «Вперёд». Состоянием владеет хозяин.
use leptos::prelude::*;
use ui_core::paging::{has_next, has_prev};

/// Размеры страницы по умолчанию.
const SIZES: &[u32] = &[20, 50, 100];

/// Пейджер. `caption` - готовая подпись («1–20 из 135» или «Страница 2 из 7»); о переходах и смене размера
/// пейджер сообщает колбэками, сам ничего не хранит. `testid` - префикс `data-testid` частей
/// (`<префикс>-pager`, `-page-info`, `-page-size`, `-prev`, `-next`).
#[component]
pub fn Pager(
    #[prop(into)] page: Signal<u32>,
    #[prop(into)] pages: Signal<u32>,
    #[prop(into)] caption: Signal<String>,
    on_page: Callback<u32>,
    /// Текущий размер страницы; без него выбора размера нет.
    #[prop(optional, into)]
    size: Option<Signal<u32>>,
    #[prop(optional)] on_size: Option<Callback<u32>>,
    #[prop(default = SIZES)] sizes: &'static [u32],
    #[prop(default = "pager")] testid: &'static str,
) -> impl IntoView {
    let size_select = size.zip(on_size).map(|(size, on_size)| {
        view! {
            <label class="grid__size">
                "Строк"
                <select
                    data-testid=format!("{testid}-page-size")
                    on:change=move |ev| {
                        let value: u32 = event_target_value(&ev).parse().unwrap_or(sizes[0]);
                        on_size.run(value);
                    }
                >
                    {sizes
                        .iter()
                        .map(|&s| {
                            view! { <option value=s.to_string() selected=move || s == size.get()>{s.to_string()}</option> }
                        })
                        .collect_view()}
                </select>
            </label>
        }
    });
    view! {
        <div class="grid__pager" data-testid=format!("{testid}-pager")>
            <span class="muted" data-testid=format!("{testid}-page-info")>{move || caption.get()}</span>
            <span class="spacer"></span>
            {size_select}
            <button
                type="button"
                class="btn"
                data-testid=format!("{testid}-prev")
                disabled=move || !has_prev(page.get())
                on:click=move |_| on_page.run(page.get_untracked().saturating_sub(1))
            >
                "Назад"
            </button>
            <button
                type="button"
                class="btn"
                data-testid=format!("{testid}-next")
                disabled=move || !has_next(page.get(), pages.get())
                on:click=move |_| on_page.run(page.get_untracked() + 1)
            >
                "Вперёд"
            </button>
        </div>
    }
}
