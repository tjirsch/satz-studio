//! The composer: the message, Send and Cancel, the model, the effort, and the chips
//! for what the provider lacks.

use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;
use satz_studio_core::llm::Effort;

use super::actions::ChatAction;
use super::state::{AgentStatus, ChatStore, ChatStoreStoreExt};
use crate::components::{
    Button, ButtonVariant, Chip, ChipKind, Icon, Segment, SegmentedButton, TextField,
};

const EFFORTS: [(Effort, &str); 5] = [
    (Effort::Low, "low"),
    (Effort::Medium, "medium"),
    (Effort::High, "high"),
    (Effort::Xhigh, "xhigh"),
    (Effort::Max, "max"),
];

fn effort_key(effort: Effort) -> &'static str {
    EFFORTS
        .iter()
        .find(|(e, _)| *e == effort)
        .map(|(_, key)| *key)
        .expect("every effort has a key")
}

fn effort_from(key: &str) -> Option<Effort> {
    EFFORTS.iter().find(|(_, k)| *k == key).map(|(e, _)| *e)
}

#[component]
pub fn Composer() -> Element {
    let chat = use_context::<Store<ChatStore>>();
    let handle = use_coroutine_handle::<ChatAction>();
    let mut draft = use_signal(String::new);
    let mut model_field = use_signal(|| chat.model().peek().clone());
    // the store's model moves on a resume and on a retry: the field follows it
    use_effect(move || model_field.set(chat.model().cloned()));

    let busy = chat.busy().cloned();
    let ready = chat.agent().cloned() == AgentStatus::Ready;
    let caps = chat.capabilities().cloned();
    let effort = chat.effort().cloned();
    let can_send = ready && !busy && !draft().trim().is_empty();
    let mut send = move || {
        let text = draft().trim().to_string();
        if text.is_empty() {
            return;
        }
        handle.send(ChatAction::Send(text));
        draft.set(String::new());
    };

    rsx! {
        div { class: "chat__composer",
            if !caps.tools {
                div { class: "chat__caveat",
                    Icon { name: "build_circle", size: 20 }
                    span { "This provider does not call tools: the satz tools are unavailable to the model, and it answers from the prompt alone." }
                }
            }
            textarea {
                class: "chat__input",
                rows: 3,
                placeholder: "Ask about this estate. Enter sends, Shift+Enter starts a new line.",
                value: "{draft}",
                disabled: !ready,
                spellcheck: "false",
                oninput: move |e: FormEvent| draft.set(e.value()),
                onkeydown: move |e: KeyboardEvent| {
                    if e.key() == Key::Enter && !e.modifiers().shift() {
                        e.prevent_default();
                        if can_send {
                            send();
                        }
                    }
                },
            }
            div { class: "chat__controls", class: if busy { "chat__controls--locked" },
                TextField {
                    label: "Model",
                    value: model_field(),
                    monospace: true,
                    class: "chat__model",
                    supporting: "Enter applies it; a different model starts a new transcript",
                    oninput: move |v: String| model_field.set(v),
                    onenter: move |_| handle.send(ChatAction::SetModel(model_field())),
                }
                if caps.effort {
                    div { class: "chat__effort",
                        span { class: "chat__control-label", "Effort" }
                        SegmentedButton {
                            options: EFFORTS.iter().map(|(_, key)| Segment::new(*key, *key)).collect(),
                            selected: effort_key(effort).to_string(),
                            onselect: move |key: String| {
                                if let Some(effort) = effort_from(&key) {
                                    handle.send(ChatAction::SetEffort(effort));
                                }
                            },
                        }
                    }
                }
                div { class: "chat__caps",
                    if !caps.thinking {
                        Chip { kind: ChipKind::Assist, icon: "psychology_alt", label: "no thinking" }
                    }
                    if !caps.effort {
                        Chip { kind: ChipKind::Assist, icon: "speed", label: "no effort" }
                    }
                    if !caps.cache_control {
                        Chip { kind: ChipKind::Assist, icon: "cached", label: "no caching" }
                    }
                }
            }
            div { class: "chat__actions chat__actions--end",
                Button { variant: ButtonVariant::Outlined, icon: "stop", disabled: !busy, onclick: move |_| handle.send(ChatAction::Cancel), "Cancel" }
                Button { variant: ButtonVariant::Filled, icon: "send", disabled: !can_send, onclick: move |_| send(), "Send" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effort_has_a_key_that_reads_back() {
        for (effort, key) in EFFORTS {
            assert_eq!(effort_key(effort), key);
            assert_eq!(effort_from(key), Some(effort));
        }
        assert_eq!(effort_from("ultra"), None);
    }
}
