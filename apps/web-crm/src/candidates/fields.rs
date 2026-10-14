//! Поле формы с подписью и сообщением об ошибке под ним.
use leptos::prelude::*;

#[component]
pub fn Field(
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
            {move || {
                error
                    .get()
                    .map(|message| {
                        view! {
                            <span class="field-error" role="alert" data-testid=format!("error-{name}")>
                                {message}
                            </span>
                        }
                    })
            }}
        </label>
    }
}
