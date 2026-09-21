//! One card per tool call: the name, the status, the duration and one line about the
//! result; a progress ring while the call runs. The input and the result JSON are the
//! debug log's, and with the debug panel on the card links to its entry.

use dioxus::prelude::*;

use super::state::{ChatStore, ChatStoreStoreExt, ToolCard};
use crate::components::{Card, CardVariant, CircularProgress, Icon, IconButton};
use crate::state::{AppStore, AppStoreStoreExt};

#[component]
pub fn ToolCardView(card: ToolCard) -> Element {
    let app = use_context::<Store<AppStore>>();
    let chat = use_context::<Store<ChatStore>>();
    let debug_on = app.settings().read().chat_debug_log;
    let running = card.result.is_none();
    let failed = card.result.as_ref().is_some_and(|r| r.is_error);
    let shown = card.shown();
    let id = card.id.clone();
    rsx! {
        Card {
            variant: CardVariant::Outlined,
            class: if failed { "chat__tool chat__tool--error" } else { "chat__tool" },
            div { class: "chat__tool-header",
                Icon { name: "handyman", size: 20 }
                code { class: "chat__tool-name", "{shown.name}" }
                if let Some(summary) = shown.summary {
                    span {
                        class: "chat__tool-summary",
                        class: if failed { "chat__result--error" },
                        title: "{summary}",
                        "{summary}"
                    }
                } else {
                    span { class: "grow" }
                }
                if let Some(duration) = shown.duration {
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
                if debug_on {
                    IconButton {
                        icon: "data_object",
                        label: "Show this call in the debug log",
                        onclick: move |_| chat.debug_focus().set(Some(id.clone())),
                    }
                }
            }
        }
    }
}
