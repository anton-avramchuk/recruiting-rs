//! Поля формы: подпись, значение и ошибка рядом. Значением владеет хозяин формы (сигнал).
use leptos::prelude::*;

/// Сообщение об ошибке под полем.
fn field_error(error: Signal<Option<String>>, testid: Option<String>) -> impl IntoView {
    move || {
        error.get().map(|message| {
            view! {
                <span class="field-error" role="alert" data-testid=testid.clone()>
                    {message}
                </span>
            }
        })
    }
}

/// Подпись, поле ввода (дети) и сообщение об ошибке; подпись связана с полем (`label`).
#[component]
pub fn Field(
    #[prop(into)] label: String,
    /// Ошибка проверки; без неё сообщение не показывается.
    #[prop(optional, into)]
    error: Option<Signal<Option<String>>>,
    #[prop(optional, into)] error_testid: Option<String>,
    children: Children,
) -> impl IntoView {
    view! {
        <label>
            {label} {children()}
            {error.map(|error| field_error(error, error_testid))}
        </label>
    }
}

/// Строка текста с подписью и ошибкой. Поле входит в форму по `name` (для `form_fields`).
#[component]
pub fn TextField(
    #[prop(into)] label: String,
    name: &'static str,
    #[prop(optional, into)] placeholder: String,
    #[prop(optional, into)] value: String,
    #[prop(optional)] required: bool,
    #[prop(into)] error: Signal<Option<String>>,
) -> impl IntoView {
    view! {
        <label>
            {label}
            <input type="text" name=name placeholder=placeholder value=value required=required />
            {field_error(error, Some(format!("error-{name}")))}
        </label>
    }
}

/// Однострочное поле, связанное с сигналом.
#[component]
pub fn TextBox(
    value: RwSignal<String>,
    #[prop(default = "text")] kind: &'static str,
    #[prop(optional, into)] placeholder: String,
    #[prop(optional)] required: bool,
    #[prop(optional)] testid: Option<&'static str>,
    #[prop(optional)] maxlength: Option<u32>,
) -> impl IntoView {
    view! {
        <input
            type=kind
            placeholder=placeholder
            required=required
            maxlength=maxlength
            data-testid=testid
            prop:value=move || value.get()
            on:input=move |ev| value.set(event_target_value(&ev))
        />
    }
}

/// Многострочное поле.
#[component]
pub fn Textarea(
    value: RwSignal<String>,
    #[prop(default = 3)] rows: u32,
    #[prop(optional, into)] placeholder: String,
    #[prop(optional)] testid: Option<&'static str>,
) -> impl IntoView {
    view! {
        <textarea
            rows=rows
            placeholder=placeholder
            data-testid=testid
            prop:value=move || value.get()
            on:input=move |ev| value.set(event_target_value(&ev))
        ></textarea>
    }
}

/// Целое число: в поле остаются только цифры, пробелы и знак минус; разбор остаётся за хозяином.
#[component]
pub fn NumberBox(
    value: RwSignal<String>,
    #[prop(optional, into)] placeholder: String,
    #[prop(optional)] testid: Option<&'static str>,
) -> impl IntoView {
    view! {
        <input
            type="text"
            inputmode="numeric"
            placeholder=placeholder
            data-testid=testid
            prop:value=move || value.get()
            on:input=move |ev| {
                let cleaned: String = event_target_value(&ev)
                    .chars()
                    .filter(|c| c.is_ascii_digit() || c.is_whitespace() || *c == '-')
                    .collect();
                value.set(cleaned);
            }
        />
    }
}

/// Выбор даты либо даты и времени (`date`, `datetime-local`); значение - строка поля HTML.
#[component]
pub fn DatePicker(
    value: RwSignal<String>,
    /// `date` или `datetime-local`.
    #[prop(default = "date")]
    kind: &'static str,
    #[prop(optional)] required: bool,
    #[prop(optional)] testid: Option<&'static str>,
) -> impl IntoView {
    view! {
        <input
            type=kind
            required=required
            data-testid=testid
            prop:value=move || value.get()
            on:input=move |ev| value.set(event_target_value(&ev))
        />
    }
}

/// Флажок с подписью.
#[component]
pub fn Checkbox(
    checked: RwSignal<bool>,
    #[prop(into)] label: String,
    #[prop(optional)] testid: Option<&'static str>,
) -> impl IntoView {
    view! {
        <label class="check">
            <input
                type="checkbox"
                data-testid=testid
                prop:checked=move || checked.get()
                on:change=move |ev| checked.set(event_target_checked(&ev))
            />
            {label}
        </label>
    }
}

/// Выбор из списка пар «значение - подпись». Вариант с пустым значением (`empty`) означает «не выбрано».
#[component]
pub fn Dropdown(
    value: RwSignal<String>,
    #[prop(into)] options: Signal<Vec<(String, String)>>,
    /// Подпись варианта с пустым значением; без неё такого варианта нет.
    #[prop(optional)]
    empty: Option<&'static str>,
    #[prop(optional)] testid: Option<&'static str>,
    #[prop(optional)] aria_label: Option<&'static str>,
    #[prop(optional)] required: bool,
    /// Вызывается после смены значения пользователем.
    #[prop(optional)]
    on_change: Option<Callback<String>>,
) -> impl IntoView {
    view! {
        <select
            data-testid=testid
            aria-label=aria_label
            required=required
            on:change=move |ev| {
                let chosen = event_target_value(&ev);
                value.set(chosen.clone());
                if let Some(handler) = on_change {
                    handler.run(chosen);
                }
            }
        >
            {empty
                .map(|label| {
                    view! { <option value="" prop:selected=move || value.get().is_empty()>{label}</option> }
                })}
            {move || {
                options
                    .get()
                    .into_iter()
                    .map(|(option_value, label)| {
                        let current = option_value.clone();
                        view! {
                            <option value=option_value prop:selected=move || value.get() == current>
                                {label}
                            </option>
                        }
                    })
                    .collect_view()
            }}
        </select>
    }
}
