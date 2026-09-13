use dioxus::prelude::*;

/// A plain tooltip under its anchor, shown on hover and focus.
#[component]
pub fn Tooltip(text: String, #[props(default)] class: String, children: Element) -> Element {
    rsx! {
        span { class: "m-tooltip-anchor {class}",
            {children}
            span { class: "m-tooltip", role: "tooltip", "{text}" }
        }
    }
}
