//! The approval card for the write tool the agent is waiting on: what the call will do
//! in words — the tool, the estate, the arguments as a short list, "no arguments" for a
//! call without any — and the three answers of ADR 0005. The input JSON is the debug
//! log's.

use dioxus::prelude::*;
use satz_studio_core::llm::Approval;

use super::actions::ChatAction;
use super::state::{ChatStore, ChatStoreStoreExt};
use super::summary::approval_text;
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
    let open = app.open().cloned();
    let tool = open
        .as_ref()
        .and_then(|o| o.session.tool_info(&pending.name).cloned());
    // a destructive tool asks every time: "for the session" would not cover it
    let destructive = tool
        .as_ref()
        .and_then(|t| t.annotations.destructive)
        .unwrap_or(false);
    let does = tool
        .as_ref()
        .and_then(|t| t.description.lines().map(str::trim).find(|l| !l.is_empty()))
        .map(str::to_string);
    let estate = open
        .as_ref()
        .and_then(|o| o.session.main.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "the open estate".to_string());
    let text = approval_text(&pending.name, &estate, &pending.input);
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
            if let Some(does) = does {
                p { class: "chat__approval-does", "{does}" }
            }
            p { class: "chat__approval-sentence", "{text.sentence}" }
            if !text.arguments.is_empty() {
                ul { class: "chat__approval-args",
                    for (i, line) in text.arguments.iter().enumerate() {
                        li { key: "{i}", "{line}" }
                    }
                }
            }
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
