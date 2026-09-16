use dioxus::prelude::*;

use super::{Badge, Icon};

/// Primary tabs: one row of destinations inside a view, the active one carrying the
/// indicator. <https://m3.material.io/components/tabs/specs>
#[component]
pub fn Tabs(children: Element) -> Element {
    rsx! {
        div { class: "m-tabs", role: "tablist", {children} }
    }
}

/// One tab: an icon above its label, a badge count, the active indicator under it.
#[component]
pub fn Tab(
    label: String,
    #[props(default)] icon: String,
    #[props(default)] selected: bool,
    #[props(default)] badge: usize,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "m-tab",
            class: if selected { "m-tab--active" },
            role: "tab",
            "aria-selected": "{selected}",
            onclick: move |e| onclick.call(e),
            span { class: "m-tab__content",
                if !icon.is_empty() {
                    Badge { count: badge,
                        Icon { name: icon.clone(), filled: selected, size: 22 }
                    }
                }
                span { class: "m-tab__label", "{label}" }
            }
            span { class: "m-tab__indicator" }
        }
    }
}
