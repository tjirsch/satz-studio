use dioxus::prelude::*;

use super::Icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Filled,
    Tonal,
    Outlined,
    Text,
    Elevated,
}

impl ButtonVariant {
    fn class(self) -> &'static str {
        match self {
            ButtonVariant::Filled => "m-button--filled",
            ButtonVariant::Tonal => "m-button--tonal",
            ButtonVariant::Outlined => "m-button--outlined",
            ButtonVariant::Text => "m-button--text",
            ButtonVariant::Elevated => "m-button--elevated",
        }
    }
}

/// A common button: 40px high, full shape at rest, medium shape while pressed.
#[component]
pub fn Button(
    #[props(default)] variant: ButtonVariant,
    #[props(default)] icon: String,
    #[props(default)] disabled: bool,
    #[props(default)] class: String,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "m-button {variant.class()} {class}",
            disabled,
            onclick: move |e| onclick.call(e),
            if !icon.is_empty() {
                Icon { name: icon.clone(), size: 18 }
            }
            span { class: "m-button__label", {children} }
        }
    }
}

/// Buttons side by side; `connected` fuses them into one shape with a 2px gap.
#[component]
pub fn ButtonGroup(
    #[props(default)] connected: bool,
    #[props(default)] class: String,
    children: Element,
) -> Element {
    rsx! {
        div { class: "m-button-group {class}", class: if connected { "m-button-group--connected" }, role: "group", {children} }
    }
}
