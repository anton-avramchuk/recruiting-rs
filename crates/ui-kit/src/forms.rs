//! Значения полей формы для обработчиков `on:submit` (замена `ActionForm`).
use std::collections::HashMap;

/// Поля формы: имя -> значение (для повторяющихся имён остаётся последнее).
pub type FormFields = HashMap<String, String>;

/// Читает поля формы, из которой пришло событие отправки. На сервере форма не отправляется,
/// поэтому там результат пуст.
#[cfg(feature = "hydrate")]
pub fn form_fields(ev: &leptos::ev::SubmitEvent) -> FormFields {
    use leptos::wasm_bindgen::JsCast;

    let mut fields = FormFields::new();
    let Some(form) = ev
        .target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlFormElement>().ok())
    else {
        return fields;
    };
    let Ok(data) = web_sys::FormData::new_with_form(&form) else {
        return fields;
    };
    let Ok(Some(entries)) = js_sys::try_iter(&data) else {
        return fields;
    };
    for entry in entries.flatten() {
        let pair = js_sys::Array::from(&entry);
        if let (Some(name), Some(value)) = (pair.get(0).as_string(), pair.get(1).as_string()) {
            fields.insert(name, value);
        }
    }
    fields
}

#[cfg(not(feature = "hydrate"))]
pub fn form_fields(_ev: &leptos::ev::SubmitEvent) -> FormFields {
    FormFields::new()
}
