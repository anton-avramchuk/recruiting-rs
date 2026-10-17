//! Модальные окна и боковая панель. Открытием владеет вызывающий код.
use leptos::{ev::KeyboardEvent, prelude::*};
use ui_core::closes_dialog;

use crate::{Notice, NoticeKind};

/// Модальное окно: подложка, контейнер с ролью и подписью для скринридеров.
///
/// Закрытие (`on_close`) - щелчок по подложке и, если фокус внутри окна, Esc; оба действия отключаются,
/// пока `closable` ложно (идёт операция). Окна с собственной раскладкой Esc (палитра) отключают
/// `close_on_esc` и обрабатывают клавишу сами, в том числе через `on_keydown`.
#[component]
pub fn Dialog(
    #[prop(into)] open: Signal<bool>,
    on_close: Callback<()>,
    /// Подпись окна для скринридеров.
    label: &'static str,
    /// `dialog` либо `alertdialog` (подтверждение необратимого действия).
    #[prop(default = "dialog")]
    role: &'static str,
    /// Класс контейнера; положение и размер задаёт CSS класса.
    #[prop(default = "dialog card")]
    class: &'static str,
    #[prop(optional)] testid: Option<&'static str>,
    #[prop(optional)] scrim_testid: Option<&'static str>,
    #[prop(optional, into)] closable: Option<Signal<bool>>,
    #[prop(default = true)] close_on_esc: bool,
    /// Нажатия клавиш внутри окна (до проверки Esc).
    #[prop(optional)]
    on_keydown: Option<Callback<KeyboardEvent>>,
    children: ChildrenFn,
) -> impl IntoView {
    let can_close = move || closable.is_none_or(|c| c.get());
    let close = move || {
        if can_close() {
            on_close.run(());
        }
    };
    view! {
        <Show when=move || open.get()>
            <div class="scrim" data-testid=scrim_testid on:click=move |_| close()></div>
            <div
                class=class
                role=role
                aria-modal="true"
                aria-label=label
                data-testid=testid
                on:keydown=move |ev| {
                    if let Some(handler) = on_keydown {
                        handler.run(ev.clone());
                    }
                    if close_on_esc && !ev.default_prevented() && closes_dialog(&ev.key()) {
                        close();
                    }
                }
            >
                {children()}
            </div>
        </Show>
    }
}

/// Боковая панель справа с подложкой. Esc панель не закрывает сама: у содержимого может быть своя
/// раскладка клавиши (форма внутри панели), её ведёт хозяин.
#[component]
pub fn Drawer(
    #[prop(into)] open: Signal<bool>,
    on_close: Callback<()>,
    label: &'static str,
    #[prop(optional)] testid: Option<&'static str>,
    children: ChildrenFn,
) -> impl IntoView {
    view! {
        <Show when=move || open.get()>
            <div class="backdrop" on:click=move |_| on_close.run(())></div>
            <aside class="drawer" role="dialog" aria-label=label data-testid=testid>
                {children()}
            </aside>
        </Show>
    }
}

/// Диалог подтверждения необратимого действия. Открытием и закрытием управляет вызывающий
/// код через `open`; закрытие по «Отмена», Esc и щелчку по подложке не вызывает `on_confirm`.
/// Пока идёт запрос (`busy`), кнопки заблокированы; `error` показывается в диалоге.
#[component]
pub fn ConfirmDialog(
    open: RwSignal<bool>,
    /// Название записи для вопроса.
    #[prop(into)]
    label: Signal<String>,
    on_confirm: Callback<()>,
    #[prop(optional, into)] busy: Option<Signal<bool>>,
    #[prop(optional, into)] error: Option<Signal<Option<String>>>,
    #[prop(default = "Удалить")] confirm_text: &'static str,
) -> impl IntoView {
    let busy = move || busy.is_some_and(|b| b.get());
    // фокус на «Отмена»: Esc работает сразу, а случайный Enter ничего не удаляет
    let cancel_button = NodeRef::<leptos::html::Button>::new();
    Effect::new(move |_| {
        if open.get()
            && let Some(button) = cancel_button.get()
        {
            let _ = button.focus();
        }
    });
    let cancel = move || {
        if !busy() {
            open.set(false);
        }
    };
    view! {
        <Dialog
            open=open
            on_close=Callback::new(move |()| cancel())
            label="Подтверждение"
            role="alertdialog"
            testid="confirm-dialog"
            scrim_testid="confirm-backdrop"
            closable=Signal::derive(move || !busy())
        >
            <p id="confirm-question" class="dialog__question" data-testid="confirm-question">
                {move || ui_core::confirm_question(&label.get())}
            </p>
            {move || {
                error
                    .and_then(|e| e.get())
                    .map(|m| view! { <div data-testid="confirm-error"><Notice kind=NoticeKind::Error>{m}</Notice></div> })
            }}
            <div class="dialog__actions">
                <button
                    type="button"
                    class="btn"
                    node_ref=cancel_button
                    data-testid="confirm-cancel"
                    disabled=busy
                    on:click=move |_| cancel()
                >
                    "Отмена"
                </button>
                <button
                    type="button"
                    class="btn btn--danger"
                    data-testid="confirm-ok"
                    disabled=busy
                    on:click=move |_| on_confirm.run(())
                >
                    {confirm_text}
                </button>
            </div>
        </Dialog>
    }
}
