use dioxus::prelude::*;

use super::{Chip, ChipKind, TextField};

/// A list of short values as input chips, each removable, with a field that adds one
/// on Enter or on blur; a comma in the typed text adds several. `onchange` gets the
/// whole list after every change.
#[component]
pub fn ChipList(
    label: String,
    items: Vec<String>,
    onchange: EventHandler<Vec<String>>,
    #[props(default)] supporting: String,
    #[props(default)] error: bool,
    #[props(default)] disabled: bool,
    #[props(default)] class: String,
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
        div { class: "chip-list {class}", class: if disabled { "chip-list--disabled" },
            if !items.is_empty() {
                div { class: "chip-list__chips",
                    for (i, item) in items.iter().enumerate() {
                        {
                            let rest = items.clone();
                            rsx! {
                                if disabled {
                                    Chip { key: "{i}-{item}", kind: ChipKind::Input, label: item.clone() }
                                } else {
                                    Chip {
                                        key: "{i}-{item}",
                                        kind: ChipKind::Input,
                                        label: item.clone(),
                                        onremove: move |_| {
                                            let mut next = rest.clone();
                                            next.remove(i);
                                            onchange.call(next);
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }
            TextField {
                label,
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
