use dioxus::prelude::*;

use super::Icon;

/// An outlined text field: floating label, optional leading icon, supporting text,
/// error state. `oninput` gets the new value; `onenter` fires on Enter.
#[component]
pub fn TextField(
    label: String,
    value: String,
    oninput: EventHandler<String>,
    #[props(default)] supporting: String,
    #[props(default)] error: bool,
    #[props(default)] placeholder: String,
    #[props(default)] disabled: bool,
    #[props(default)] monospace: bool,
    #[props(default)] password: bool,
    #[props(default)] leading_icon: String,
    #[props(default)] class: String,
    #[props(default)] onenter: Option<EventHandler<()>>,
) -> Element {
    let populated = !value.is_empty() || !placeholder.is_empty();
    rsx! {
        label {
            class: "m-text-field {class}",
            class: if populated { "m-text-field--populated" },
            class: if error { "m-text-field--error" },
            class: if disabled { "m-text-field--disabled" },
            class: if !leading_icon.is_empty() { "m-text-field--with-icon" },
            div { class: "m-text-field__outline",
                if !leading_icon.is_empty() {
                    Icon { name: leading_icon.clone(), size: 20, class: "m-text-field__icon" }
                }
                input {
                    class: "m-text-field__input",
                    class: if monospace { "m-text-field__input--mono" },
                    r#type: if password { "password" } else { "text" },
                    value: "{value}",
                    placeholder: "{placeholder}",
                    disabled,
                    spellcheck: "false",
                    autocomplete: "off",
                    oninput: move |e: FormEvent| oninput.call(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter
                            && let Some(h) = &onenter
                        {
                            h.call(());
                        }
                    },
                }
                span { class: "m-text-field__label", "{label}" }
            }
            if !supporting.is_empty() {
                span { class: "m-text-field__supporting", "{supporting}" }
            }
        }
    }
}
