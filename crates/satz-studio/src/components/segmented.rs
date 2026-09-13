use dioxus::prelude::*;

use super::Icon;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub value: String,
    pub label: String,
    pub icon: String,
}

impl Segment {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            icon: String::new(),
        }
    }
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }
}

/// A single-select segmented button: outlined, connected, full shape at the ends; the
/// selected segment shows a check in place of its icon.
#[component]
pub fn SegmentedButton(
    options: Vec<Segment>,
    selected: String,
    onselect: EventHandler<String>,
    #[props(default)] class: String,
) -> Element {
    rsx! {
        div { class: "m-segmented {class}", role: "group",
            for option in options {
                {
                    let is_selected = option.value == selected;
                    let value = option.value.clone();
                    let icon = if is_selected { "check".to_string() } else { option.icon.clone() };
                    rsx! {
                        button {
                            key: "{option.value}",
                            r#type: "button",
                            class: "m-segmented__item",
                            class: if is_selected { "m-segmented__item--selected" },
                            "aria-pressed": if is_selected { "true" } else { "false" },
                            onclick: move |_| onselect.call(value.clone()),
                            if !icon.is_empty() {
                                Icon { name: icon.clone(), size: 18 }
                            }
                            span { "{option.label}" }
                        }
                    }
                }
            }
        }
    }
}
