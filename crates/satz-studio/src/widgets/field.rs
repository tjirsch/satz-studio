use dioxus::prelude::*;

use crate::components::input::Input;
use crate::components::label::Label;

/// One labelled line of text: the library's `Label` over its `Input`, with one line
/// under it that explains the field or says what is wrong with the value. With
/// `label_hidden` — a row that names its value at its start — the label is the input's
/// accessible name and is not drawn a second time. `oninput`
/// carries the text; `onenter` and `onblur` are the two moments a view commits at.
#[component]
pub fn Field(
    label: String,
    value: String,
    oninput: EventHandler<String>,
    #[props(default)] supporting: String,
    #[props(default)] error: bool,
    #[props(default)] placeholder: String,
    #[props(default)] disabled: bool,
    #[props(default)] monospace: bool,
    #[props(default)] password: bool,
    #[props(default)] class: String,
    #[props(default)] label_hidden: bool,
    #[props(default)] onenter: Option<EventHandler<()>>,
    #[props(default)] onblur: Option<EventHandler<()>>,
) -> Element {
    let id = use_hook(field_id);
    rsx! {
        div { class: "field {class}",
            if !label_hidden {
                Label { html_for: id.clone(), "{label}" }
            }
            Input {
                id,
                "aria-label": label_hidden.then(|| label.clone()),
                class: if monospace { "field__input field__input--mono" } else { "field__input" },
                r#type: if password { "password" } else { "text" },
                value: "{value}",
                placeholder: "{placeholder}",
                disabled,
                "spellcheck": "false",
                autocomplete: "off",
                "aria-invalid": if error { "true" } else { "false" },
                oninput: move |e: FormEvent| oninput.call(e.value()),
                onkeydown: move |e: KeyboardEvent| {
                    if e.key() == Key::Enter
                        && let Some(h) = &onenter
                    {
                        h.call(());
                    }
                },
                onblur: move |_| {
                    if let Some(h) = &onblur {
                        h.call(());
                    }
                },
            }
            if !supporting.is_empty() {
                p { class: "field__supporting", class: if error { "field__supporting--error" }, "{supporting}" }
            }
        }
    }
}

/// An id for one field of this window, so its label can name its input.
pub(super) fn field_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("field-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}
