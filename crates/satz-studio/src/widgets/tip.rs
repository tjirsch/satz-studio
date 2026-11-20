use dioxus::prelude::*;

use crate::components::tooltip::{Tooltip, TooltipContent, TooltipTrigger};

/// The library's `Tooltip` around a control: `text` shows on hover and focus. The trigger
/// is a `span` rather than the library's default `button`, so a button inside it is not
/// nested in another.
#[component]
pub fn Tip(text: String, children: Element) -> Element {
    rsx! {
        Tooltip {
            TooltipTrigger {
                r#as: move |attributes: Vec<Attribute>| rsx! {
                    span { ..attributes, {children.clone()} }
                },
            }
            TooltipContent { "{text}" }
        }
    }
}
