//! The conversation: the completed turns, the turn being streamed, and the notice for
//! a turn that did not reach the transcript. Text is shown as plain text with its
//! paragraphs; there is no markdown renderer.

use dioxus::prelude::*;

use super::actions::ChatAction;
use super::state::{AssistantTurn, Block, ChatStore, ChatStoreStoreExt, Notice, TurnView};
use super::tool_card::ToolCardView;
use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, CircularProgress, Icon,
};
use satz_studio_core::llm::StopReason;

/// Keep the list at its end while it grows, unless the reader scrolled up.
const FOLLOW: &str = r"
const el = document.getElementById('chat-list');
if (el) {
  if (!el.dataset.follow) {
    el.dataset.follow = 'true';
    el.addEventListener('scroll', () => {
      el.dataset.follow = (el.scrollHeight - el.scrollTop - el.clientHeight < 80) ? 'true' : 'false';
    });
  }
  if (el.dataset.follow === 'true') el.scrollTop = el.scrollHeight;
}";

#[component]
pub fn TranscriptList() -> Element {
    let chat = use_context::<Store<ChatStore>>();
    let turns = chat.turns().cloned();
    let idle = turns.is_empty() && chat.streaming().is_none();
    use_effect(move || {
        let _ = chat.turns().len();
        let _ = chat.streaming().read();
        let _ = chat.pending().is_some();
        let _ = document::eval(FOLLOW);
    });
    rsx! {
        div { id: "chat-list", class: "chat__list",
            if idle {
                p { class: "chat__hint", "Ask about this estate. The model reads it through the satz tools; a call that writes waits for your approval." }
            }
            for (i, turn) in turns.iter().enumerate() {
                match turn {
                    TurnView::User { text } => rsx! {
                        div { key: "{i}", class: "chat__user",
                            p { class: "chat__text", "{text}" }
                        }
                    },
                    TurnView::Assistant(turn) => rsx! {
                        AssistantTurnView { key: "{i}", turn: turn.clone(), live: false }
                    },
                }
            }
            StreamingTurn {}
        }
    }
}

/// The assistant turn being streamed; re-renders on every delta, alone.
#[component]
fn StreamingTurn() -> Element {
    let chat = use_context::<Store<ChatStore>>();
    match chat.streaming().cloned() {
        Some(turn) => rsx! { AssistantTurnView { turn, live: true } },
        None => rsx! {},
    }
}

#[component]
fn AssistantTurnView(turn: AssistantTurn, live: bool) -> Element {
    let last = turn.blocks.len().saturating_sub(1);
    let limit = matches!(
        turn.stop_reason,
        Some(StopReason::MaxTokens | StopReason::StopSequence)
    );
    rsx! {
        div { class: "chat__assistant",
            for (i, block) in turn.blocks.iter().enumerate() {
                BlockView { key: "{i}", block: block.clone(), open_thinking: live && i == last }
            }
            if live {
                div { class: "chat__live",
                    CircularProgress { size: 20 }
                    span { "request {turn.requests}" }
                }
            } else if limit {
                Chip { kind: ChipKind::Assist, icon: "block", label: "stopped at the output limit; the answer is cut", error: true }
            }
        }
    }
}

#[component]
fn BlockView(block: Block, open_thinking: bool) -> Element {
    match block {
        Block::Text(text) => rsx! { p { class: "chat__text", "{text}" } },
        Block::Thinking(text) => rsx! { ThinkingCard { text, open: open_thinking } },
        Block::RedactedThinking => rsx! {
            Chip { kind: ChipKind::Assist, icon: "visibility_off", label: "thinking redacted by the API" }
        },
        Block::Tool(card) => rsx! { ToolCardView { card } },
        Block::Fallback { from, to } => rsx! {
            Chip { kind: ChipKind::Assist, icon: "alt_route", label: "served by {to} after {from} declined" }
        },
        Block::Other(kind) => rsx! {
            Chip { kind: ChipKind::Assist, icon: "help", label: "a {kind} block this view does not show" }
        },
    }
}

/// The summarised thinking, collapsed to its first line; open while it streams.
#[component]
fn ThinkingCard(text: String, open: bool) -> Element {
    let mut expanded = use_signal(|| open);
    let peek = text.lines().next().unwrap_or_default().to_string();
    rsx! {
        div { class: "chat__thinking",
            button {
                r#type: "button",
                class: "chat__thinking-header",
                "aria-expanded": if expanded() { "true" } else { "false" },
                onclick: move |_| expanded.toggle(),
                Icon { name: "psychology", size: 18 }
                span { class: "chat__thinking-title", "Thinking" }
                if !expanded() {
                    span { class: "chat__thinking-peek", "{peek}" }
                }
                span { class: "grow" }
                Icon { name: if expanded() { "expand_less" } else { "expand_more" }, size: 18 }
            }
            if expanded() {
                p { class: "chat__text chat__thinking-body", "{text}" }
            }
        }
    }
}

/// How the last turn ended when it did not reach the transcript, with the message
/// that began it and the ways to send it again.
#[component]
pub fn NoticeCard() -> Element {
    let chat = use_context::<Store<ChatStore>>();
    let handle = use_coroutine_handle::<ChatAction>();
    let Some(notice) = chat.notice().cloned() else {
        return rsx! {};
    };
    let user_text = notice.user_text().to_string();
    let (icon, title, detail, category, retry_on) = match &notice {
        Notice::Refused {
            category,
            explanation,
            recommended_model,
            ..
        } => (
            "gpp_bad",
            "The model refused this turn".to_string(),
            explanation.clone(),
            category.clone(),
            recommended_model.clone(),
        ),
        Notice::Failed { error, .. } => (
            "error",
            "The turn failed".to_string(),
            Some(error.clone()),
            None,
            None,
        ),
        Notice::Cancelled { .. } => (
            "cancel",
            "The turn was cancelled".to_string(),
            None,
            None,
            None,
        ),
    };
    let retry_text = user_text.clone();
    let switch_text = user_text.clone();
    rsx! {
        Card { variant: CardVariant::Filled, class: "chat__notice",
            div { class: "chat__notice-header",
                Icon { name: icon, size: 22, filled: true }
                span { class: "chat__notice-title", "{title}" }
                if let Some(category) = category {
                    Chip { kind: ChipKind::Assist, label: category, error: true }
                }
            }
            if let Some(detail) = detail {
                p { class: "chat__notice-detail", "{detail}" }
            }
            p { class: "chat__notice-label", "Your message was not added to the transcript:" }
            blockquote { class: "chat__text chat__notice-quote", "{user_text}" }
            div { class: "chat__actions",
                Button { variant: ButtonVariant::Tonal, icon: "replay", onclick: move |_| handle.send(ChatAction::Send(retry_text.clone())), "Retry" }
                if let Some(model) = retry_on {
                    {
                        let target = model.clone();
                        rsx! {
                            Button {
                                variant: ButtonVariant::Filled,
                                icon: "swap_horiz",
                                onclick: move |_| {
                                    handle.send(ChatAction::SetModel(target.clone()));
                                    handle.send(ChatAction::Send(switch_text.clone()));
                                },
                                "Retry on {model} (new transcript)"
                            }
                        }
                    }
                }
                Button { variant: ButtonVariant::Text, onclick: move |_| chat.notice().set(None), "Dismiss" }
            }
        }
    }
}
