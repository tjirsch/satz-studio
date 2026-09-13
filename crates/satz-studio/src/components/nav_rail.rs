use dioxus::prelude::*;

use super::{Badge, Icon};

/// The navigation rail: a FAB slot at the top, destinations below, a footer at the end.
#[component]
pub fn NavRail(
    #[props(default)] fab: Option<Element>,
    #[props(default)] footer: Option<Element>,
    children: Element,
) -> Element {
    rsx! {
        nav { class: "m-nav-rail", "aria-label": "views",
            if let Some(fab) = fab {
                div { class: "m-nav-rail__fab", {fab} }
            }
            div { class: "m-nav-rail__destinations", {children} }
            if let Some(footer) = footer {
                div { class: "m-nav-rail__footer", {footer} }
            }
        }
    }
}

/// One destination: icon in a pill indicator when active, label below, a badge count.
#[component]
pub fn NavRailItem(
    icon: String,
    label: String,
    #[props(default)] selected: bool,
    #[props(default)] badge: usize,
    #[props(default)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "m-nav-rail__item",
            class: if selected { "m-nav-rail__item--active" },
            "aria-current": if selected { "page" },
            disabled,
            onclick: move |e| onclick.call(e),
            span { class: "m-nav-rail__indicator",
                Badge { count: badge,
                    Icon { name: icon.clone(), filled: selected }
                }
            }
            span { class: "m-nav-rail__label", "{label}" }
        }
    }
}
