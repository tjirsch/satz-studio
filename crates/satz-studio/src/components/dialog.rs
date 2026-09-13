use dioxus::prelude::*;

use super::Icon;

/// A basic dialog over a scrim: headline, optional icon, body, actions. Clicking the
/// scrim or pressing Escape dismisses it.
#[component]
pub fn Dialog(
    open: bool,
    title: String,
    ondismiss: EventHandler<()>,
    #[props(default)] icon: String,
    #[props(default)] actions: Option<Element>,
    children: Element,
) -> Element {
    if !open {
        return rsx! {};
    }
    rsx! {
        div {
            class: "m-scrim",
            onclick: move |_| ondismiss.call(()),
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    ondismiss.call(());
                }
            },
            div {
                class: "m-dialog",
                role: "dialog",
                "aria-modal": "true",
                "aria-label": "{title}",
                onclick: move |e| e.stop_propagation(),
                if !icon.is_empty() {
                    Icon { name: icon.clone(), class: "m-dialog__icon" }
                }
                h2 { class: "m-dialog__headline", "{title}" }
                div { class: "m-dialog__body", {children} }
                if let Some(actions) = actions {
                    div { class: "m-dialog__actions", {actions} }
                }
            }
        }
    }
}
