use dioxus::prelude::*;

use super::Icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconButtonVariant {
    #[default]
    Standard,
    Filled,
    Tonal,
    Outlined,
}

impl IconButtonVariant {
    fn class(self) -> &'static str {
        match self {
            IconButtonVariant::Standard => "m-icon-button--standard",
            IconButtonVariant::Filled => "m-icon-button--filled",
            IconButtonVariant::Tonal => "m-icon-button--tonal",
            IconButtonVariant::Outlined => "m-icon-button--outlined",
        }
    }
}

/// A 40px icon button; `label` is its accessible name and tooltip; `selected` fills
/// the glyph and, for the filled and tonal variants, swaps the container colour.
#[component]
pub fn IconButton(
    icon: String,
    label: String,
    #[props(default)] variant: IconButtonVariant,
    #[props(default)] selected: bool,
    #[props(default)] disabled: bool,
    #[props(default)] class: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "m-icon-button {variant.class()} {class}",
            class: if selected { "m-icon-button--selected" },
            "aria-label": "{label}",
            "aria-pressed": if selected { "true" } else { "false" },
            title: "{label}",
            disabled,
            onclick: move |e| onclick.call(e),
            Icon { name: icon.clone(), filled: selected }
        }
    }
}
