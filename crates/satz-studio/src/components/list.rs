use dioxus::prelude::*;

/// A list container.
#[component]
pub fn List(#[props(default)] class: String, children: Element) -> Element {
    rsx! {
        ul { class: "m-list {class}", role: "list", {children} }
    }
}

/// One list item: headline, supporting text, leading and trailing slots.
#[component]
pub fn ListItem(
    headline: String,
    #[props(default)] supporting: String,
    #[props(default)] leading: Option<Element>,
    #[props(default)] trailing: Option<Element>,
    #[props(default)] selected: bool,
    #[props(default)] class: String,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    rsx! {
        li {
            class: "m-list-item {class}",
            class: if selected { "m-list-item--selected" },
            class: if onclick.is_some() { "m-list-item--clickable" },
            role: if onclick.is_some() { "button" } else { "listitem" },
            tabindex: if onclick.is_some() { "0" } else { "-1" },
            onclick: move |e| {
                if let Some(h) = &onclick {
                    h.call(e);
                }
            },
            if let Some(leading) = leading {
                span { class: "m-list-item__leading", {leading} }
            }
            span { class: "m-list-item__text",
                span { class: "m-list-item__headline", "{headline}" }
                if !supporting.is_empty() {
                    span { class: "m-list-item__supporting", "{supporting}" }
                }
            }
            if let Some(trailing) = trailing {
                span { class: "m-list-item__trailing", {trailing} }
            }
        }
    }
}
