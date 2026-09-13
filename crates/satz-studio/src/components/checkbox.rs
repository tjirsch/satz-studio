use dioxus::prelude::*;

use super::Icon;

/// A checkbox: 18px box in a 40px target.
#[component]
pub fn Checkbox(
    checked: bool,
    onchange: EventHandler<bool>,
    #[props(default)] label: String,
    #[props(default)] disabled: bool,
) -> Element {
    rsx! {
        label { class: "m-checkbox-row", class: if disabled { "m-checkbox-row--disabled" },
            button {
                r#type: "button",
                class: "m-checkbox",
                class: if checked { "m-checkbox--checked" },
                role: "checkbox",
                "aria-checked": if checked { "true" } else { "false" },
                disabled,
                onclick: move |_| onchange.call(!checked),
                span { class: "m-checkbox__box",
                    if checked {
                        Icon { name: "check", size: 16 }
                    }
                }
            }
            if !label.is_empty() {
                span { class: "m-checkbox-row__label", "{label}" }
            }
        }
    }
}
