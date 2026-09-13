use dioxus::prelude::*;

/// A radio button: 20px ring in a 40px target; `onselect` fires when it is chosen.
#[component]
pub fn Radio(
    checked: bool,
    onselect: EventHandler<()>,
    #[props(default)] label: String,
    #[props(default)] disabled: bool,
) -> Element {
    rsx! {
        label { class: "m-radio-row", class: if disabled { "m-radio-row--disabled" },
            button {
                r#type: "button",
                class: "m-radio",
                class: if checked { "m-radio--checked" },
                role: "radio",
                "aria-checked": if checked { "true" } else { "false" },
                disabled,
                onclick: move |_| onselect.call(()),
                span { class: "m-radio__ring", span { class: "m-radio__dot" } }
            }
            if !label.is_empty() {
                span { class: "m-radio-row__label", "{label}" }
            }
        }
    }
}
