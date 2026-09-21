//! The debug panel beside the conversation, shown while `Settings.chat_debug_log` is on:
//! every tool call of the conversation under its call id, with the input JSON, the
//! result JSON the model read, whether it was an error, how long it took and the satz
//! stderr lines that arrived during it. A tool card's link scrolls to its entry.

use dioxus::prelude::*;

use super::state::{ChatStore, ChatStoreStoreExt, DebugEvent, StderrLines};
use crate::components::{Icon, IconButton};

/// The element id of a call's entry; a call id is the API's or Claude Code's, and only
/// its letters, digits, `-` and `_` are kept.
fn entry_id(call: &str) -> String {
    let safe: String = call
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    format!("chat-debug-{safe}")
}

#[component]
pub fn DebugPanel() -> Element {
    let chat = use_context::<Store<ChatStore>>();
    let entries = chat.debug().cloned();
    let focus = chat.debug_focus().cloned();
    // a call of the running turn without its result is running; an earlier one ended
    // with its turn
    let running_from = if chat.busy().cloned() {
        chat.debug_turn().cloned()
    } else {
        entries.len()
    };
    use_effect(move || {
        if let Some(call) = chat.debug_focus().cloned() {
            let target = entry_id(&call);
            let _ = document::eval(&format!(
                "document.getElementById('{target}')?.scrollIntoView({{block: 'start', behavior: 'smooth'}});"
            ));
        }
    });
    rsx! {
        aside { class: "chat__debug", "aria-label": "Debug log",
            div { class: "chat__debug-header",
                Icon { name: "data_object", size: 20 }
                h2 { class: "chat__debug-title", "Debug log" }
                span { class: "grow" }
                span { class: "chat__debug-count", "{entries.len()} calls" }
            }
            if entries.is_empty() {
                p { class: "chat__debug-empty", "Every tool call of this conversation shows here with its input and its result." }
            }
            div { class: "chat__debug-list",
                for (i, entry) in entries.into_iter().enumerate() {
                    DebugEntry {
                        key: "{entry.id}",
                        focused: focus.as_deref() == Some(entry.id.as_str()),
                        running: i >= running_from,
                        entry,
                    }
                }
            }
        }
    }
}

#[component]
fn DebugEntry(entry: DebugEvent, focused: bool, running: bool) -> Element {
    let chat = use_context::<Store<ChatStore>>();
    let status = match &entry.result {
        None if running => "running",
        None => "ended with its turn, without a result",
        Some(r) if r.is_error => "error",
        Some(_) => "ok",
    };
    let millis = entry
        .result
        .as_ref()
        .and_then(|r| r.millis)
        .map(|ms| format!("{ms} ms"));
    rsx! {
        section {
            id: "{entry_id(&entry.id)}",
            class: "chat__debug-entry",
            class: if focused { "chat__debug-entry--focused" },
            div { class: "chat__debug-entry-header",
                code { class: "chat__tool-name", "{entry.name}" }
                span { class: "chat__debug-status", class: if status == "error" { "chat__result--error" }, "{status}" }
                if let Some(ms) = millis {
                    span { class: "chat__tool-duration", "{ms}" }
                }
                span { class: "grow" }
                if focused {
                    IconButton { icon: "close", label: "Clear the highlight", onclick: move |_| chat.debug_focus().set(None) }
                }
            }
            code { class: "chat__debug-id", "{entry.id}" }
            p { class: "chat__debug-label", "Input" }
            match &entry.input {
                Some(input) => rsx! { pre { class: "chat__json", "{input}" } },
                None => rsx! { p { class: "chat__debug-none", "not known yet" } },
            }
            p { class: "chat__debug-label", "Result" }
            match &entry.result {
                Some(result) => rsx! {
                    pre { class: "chat__json", class: if result.is_error { "chat__result--error" }, "{result.body}" }
                },
                None => rsx! { p { class: "chat__debug-none", "none" } },
            }
            p { class: "chat__debug-label", "satz stderr" }
            match &entry.stderr {
                StderrLines::Lines(lines) if lines.is_empty() => rsx! {
                    p { class: "chat__debug-none", "no lines" }
                },
                StderrLines::Lines(lines) => rsx! {
                    pre { class: "chat__json", "{lines.join(\"\\n\")}" }
                },
                StderrLines::Unseen(why) => rsx! {
                    p { class: "chat__debug-none", "{why}" }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_id_keeps_only_what_an_element_id_and_a_selector_can_carry() {
        assert_eq!(entry_id("toolu_01AbC-9"), "chat-debug-toolu_01AbC-9");
        assert_eq!(entry_id("cc'1\"<x>"), "chat-debug-cc1x");
    }
}
