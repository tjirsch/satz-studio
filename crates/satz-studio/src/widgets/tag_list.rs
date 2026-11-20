use dioxus::prelude::*;
use dioxus_icons::lucide::X;

use super::Field;
use crate::components::badge::{Badge, BadgeVariant};
use crate::components::button::{Button, ButtonSize, ButtonVariant};

/// A list of short values: one library `Badge` per value with a button that takes it
/// out, and a [`Field`] that adds one, or several separated by commas, on Enter and on
/// leaving the field.
#[component]
pub fn TagList(
    label: String,
    items: Vec<String>,
    onchange: EventHandler<Vec<String>>,
    #[props(default)] supporting: String,
    #[props(default)] error: bool,
    #[props(default)] disabled: bool,
    #[props(default)] class: String,
    #[props(default)] label_hidden: bool,
) -> Element {
    let mut typed = use_signal(String::new);
    let current = items.clone();
    let add = use_callback(move |()| {
        let text = typed();
        let added: Vec<String> = text
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if added.is_empty() {
            return;
        }
        let mut next = current.clone();
        next.extend(added);
        typed.set(String::new());
        onchange.call(next);
    });
    rsx! {
        div { class: "tag-list {class}",
            if !items.is_empty() {
                div { class: "tag-list__tags",
                    for (i, item) in items.iter().enumerate() {
                        {
                            let rest = items.clone();
                            rsx! {
                                Badge { key: "{i}-{item}", variant: BadgeVariant::Secondary,
                                    code { "{item}" }
                                    if !disabled {
                                        Button {
                                            variant: ButtonVariant::Ghost,
                                            size: ButtonSize::IconXs,
                                            "aria-label": "remove {item}",
                                            onclick: move |_| {
                                                let mut next = rest.clone();
                                                next.remove(i);
                                                onchange.call(next);
                                            },
                                            X { size: 12 }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Field {
                label,
                label_hidden,
                value: typed(),
                placeholder: "add one, or several with commas",
                supporting,
                error,
                disabled,
                monospace: true,
                oninput: move |v| typed.set(v),
                onenter: move |_| add(()),
                onblur: move |_| add(()),
            }
        }
    }
}
