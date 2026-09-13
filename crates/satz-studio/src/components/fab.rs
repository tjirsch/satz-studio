use dioxus::prelude::*;

use super::Icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FabSize {
    Small,
    #[default]
    Medium,
    Large,
}

/// A floating action button; a non-empty `label` makes it the extended FAB.
#[component]
pub fn Fab(
    icon: String,
    #[props(default)] label: String,
    #[props(default)] size: FabSize,
    #[props(default)] class: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let size_class = match size {
        FabSize::Small => "m-fab--small",
        FabSize::Medium => "m-fab--medium",
        FabSize::Large => "m-fab--large",
    };
    let icon_size = match size {
        FabSize::Large => 36,
        _ => 24,
    };
    rsx! {
        button {
            r#type: "button",
            class: "m-fab {size_class} {class}",
            class: if !label.is_empty() { "m-fab--extended" },
            "aria-label": "{label}",
            title: "{label}",
            onclick: move |e| onclick.call(e),
            Icon { name: icon.clone(), size: icon_size }
            if !label.is_empty() {
                span { class: "m-fab__label", "{label}" }
            }
        }
    }
}
