use dioxus::prelude::*;

use super::Icon;

/// A switch: 52×32 track, the handle grows from 16 to 24 when on and carries a check.
#[component]
pub fn Switch(
    checked: bool,
    onchange: EventHandler<bool>,
    #[props(default)] label: String,
    #[props(default)] disabled: bool,
    #[props(default)] class: String,
) -> Element {
    rsx! {
        label { class: "m-switch-row {class}", class: if disabled { "m-switch-row--disabled" },
            if !label.is_empty() {
                span { class: "m-switch-row__label", "{label}" }
            }
            button {
                r#type: "button",
                class: "m-switch",
                class: if checked { "m-switch--on" },
                role: "switch",
                "aria-checked": if checked { "true" } else { "false" },
                disabled,
                onclick: move |_| onchange.call(!checked),
                span { class: "m-switch__track",
                    span { class: "m-switch__handle",
                        if checked {
                            Icon { name: "check", size: 16 }
                        }
                    }
                }
            }
        }
    }
}
