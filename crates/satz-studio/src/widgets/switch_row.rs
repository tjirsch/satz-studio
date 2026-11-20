use dioxus::prelude::*;

use super::field::field_id;
use crate::components::label::Label;
use crate::components::switch::Switch;

/// The library's `Switch` with its `Label` beside it, as one row.
#[component]
pub fn SwitchRow(
    label: String,
    checked: bool,
    onchange: EventHandler<bool>,
    #[props(default)] disabled: bool,
    #[props(default)] class: String,
) -> Element {
    let id = use_hook(field_id);
    rsx! {
        div { class: "switch-row {class}",
            Switch {
                id: id.clone(),
                checked,
                disabled,
                on_checked_change: move |now: bool| onchange.call(now),
            }
            Label { html_for: id, "{label}" }
        }
    }
}
