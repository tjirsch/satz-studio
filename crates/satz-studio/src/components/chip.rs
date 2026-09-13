use dioxus::prelude::*;

use super::Icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChipKind {
    #[default]
    Assist,
    Filter,
    Input,
}

/// A 32px chip. Filter chips show a check when selected; input chips carry a remove
/// button when `onremove` is given. Without `onclick` the chip is static.
#[component]
pub fn Chip(
    label: String,
    #[props(default)] kind: ChipKind,
    #[props(default)] icon: String,
    #[props(default)] selected: bool,
    #[props(default)] error: bool,
    #[props(default)] class: String,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
    #[props(default)] onremove: Option<EventHandler<MouseEvent>>,
) -> Element {
    let kind_class = match kind {
        ChipKind::Assist => "m-chip--assist",
        ChipKind::Filter => "m-chip--filter",
        ChipKind::Input => "m-chip--input",
    };
    let leading = if kind == ChipKind::Filter && selected {
        "check".to_string()
    } else {
        icon.clone()
    };
    rsx! {
        span {
            class: "m-chip {kind_class} {class}",
            class: if selected { "m-chip--selected" },
            class: if error { "m-chip--error" },
            class: if onclick.is_none() { "m-chip--static" },
            role: if onclick.is_some() { "button" } else { "status" },
            tabindex: if onclick.is_some() { "0" } else { "-1" },
            onclick: move |e| {
                if let Some(h) = &onclick {
                    h.call(e);
                }
            },
            if !leading.is_empty() {
                Icon { name: leading.clone(), size: 18, class: "m-chip__icon" }
            }
            span { class: "m-chip__label", "{label}" }
            if let Some(remove) = onremove {
                button {
                    r#type: "button",
                    class: "m-chip__remove",
                    "aria-label": "remove {label}",
                    onclick: move |e| {
                        e.stop_propagation();
                        remove.call(e);
                    },
                    Icon { name: "close", size: 18 }
                }
            }
        }
    }
}
