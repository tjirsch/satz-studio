//! The approval card for the write tool the agent is waiting on: the name, the
//! arguments ("no arguments" for a call without any), and the three answers of ADR 0005.

use dioxus::prelude::*;
use satz_studio_core::llm::Approval;

use super::actions::ChatAction;
use super::state::{ChatStore, ChatStoreStoreExt, arguments_text};
use crate::components::{Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Icon};
use crate::state::{AppStore, AppStoreStoreExt};

#[component]
pub fn ApprovalCard() -> Element {
    let app = use_context::<Store<AppStore>>();
    let chat = use_context::<Store<ChatStore>>();
    let handle = use_coroutine_handle::<ChatAction>();
    let Some(pending) = chat.pending().cloned() else {
        return rsx! {};
    };
    // a destructive tool asks every time: "for the session" would not cover it
    let destructive = app
        .open()
        .read()
        .as_ref()
        .and_then(|o| {
            o.session
                .tool_info(&pending.name)
                .map(|t| t.annotations.destructive)
        })
        .flatten()
        .unwrap_or(false);
    let input = arguments_text(&pending.input);
    rsx! {
        Card { variant: CardVariant::Filled, class: "chat__approval",
            div { class: "chat__approval-header",
                Icon { name: "gpp_maybe", size: 24, filled: true }
                span { class: "chat__approval-title",
                    "The agent wants to run "
                    code { "{pending.name}" }
                }
                if destructive {
                    Chip { kind: ChipKind::Assist, icon: "warning", label: "destructive: asks every time", error: true }
                }
            }
            pre { class: "chat__json", "{input}" }
            div { class: "chat__actions",
                Button { variant: ButtonVariant::Filled, icon: "check", onclick: move |_| handle.send(ChatAction::Approve(Approval::Once)), "Allow once" }
                if !destructive {
                    Button { variant: ButtonVariant::Tonal, icon: "done_all", onclick: move |_| handle.send(ChatAction::Approve(Approval::ForSession)), "Allow for the session" }
                }
                Button { variant: ButtonVariant::Outlined, icon: "block", onclick: move |_| handle.send(ChatAction::Approve(Approval::Deny)), "Deny" }
            }
        }
    }
}
