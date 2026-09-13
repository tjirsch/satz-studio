//! One card per tool call: the name, the input (collapsed), the result or the error,
//! the duration; a progress ring while the call runs.

use dioxus::prelude::*;

use super::state::ToolCard;
use crate::components::{Button, ButtonVariant, Card, CardVariant, CircularProgress, Icon};

#[component]
pub fn ToolCardView(card: ToolCard) -> Element {
    let mut show_input = use_signal(|| false);
    let running = card.result.is_none();
    let failed = card.result.as_ref().is_some_and(|r| r.is_error);
    let duration = card
        .result
        .as_ref()
        .and_then(|r| r.millis)
        .map(|ms| format!("{ms} ms"));
    let input = card.input_text();
    rsx! {
        Card {
            variant: CardVariant::Outlined,
            class: if failed { "chat__tool chat__tool--error" } else { "chat__tool" },
            div { class: "chat__tool-header",
                Icon { name: "handyman", size: 20 }
                code { class: "chat__tool-name", "{card.name}" }
                span { class: "grow" }
                if let Some(duration) = duration {
                    span { class: "chat__tool-duration", "{duration}" }
                }
                if running {
                    CircularProgress { size: 20 }
                } else {
                    Icon {
                        name: if failed { "error" } else { "check_circle" },
                        size: 20,
                        filled: true,
                        class: if failed { "chat__tool-status chat__tool-status--error" } else { "chat__tool-status" },
                    }
                }
                Button {
                    variant: ButtonVariant::Text,
                    icon: if show_input() { "expand_less" } else { "expand_more" },
                    onclick: move |_| show_input.toggle(),
                    "Input"
                }
            }
            if show_input() {
                pre { class: "chat__json", "{input}" }
            }
            if let Some(result) = &card.result {
                pre { class: "chat__json", class: if result.is_error { "chat__result--error" }, "{result.body}" }
            }
        }
    }
}
