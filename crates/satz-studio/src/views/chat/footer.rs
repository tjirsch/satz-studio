//! The usage footer: the last turn's figures and the session's, cache reads included
//! — a zero after the second turn is a cache that did not hit, and it is shown as a
//! zero.

use dioxus::prelude::*;
use satz_studio_core::llm::Usage;

use super::state::{ChatStore, ChatStoreStoreExt};
use crate::components::Icon;

#[component]
pub fn UsageFooter() -> Element {
    let chat = use_context::<Store<ChatStore>>();
    let usage = chat.usage().cloned();
    let model = chat.model().cloned();
    let transcript = chat.transcript().cloned();
    let kept = match transcript {
        Some(path) => format!("kept in {}", path.display()),
        None => "not kept".to_string(),
    };
    rsx! {
        footer { class: "chat__footer",
            Icon { name: "data_usage", size: 16 }
            span { class: "chat__footer-group",
                span { class: "chat__footer-label", "this turn" }
                span { {usage_text(&usage.turn)} }
            }
            span { class: "chat__footer-group",
                span { class: "chat__footer-label", "session, {usage.turns} turns" }
                span { {usage_text(&usage.session)} }
            }
            span { class: "grow" }
            span { class: "chat__footer-group",
                code { "{model}" }
                span { class: "chat__footer-label", "{kept}" }
            }
        }
    }
}

/// `in 1200 · out 80 · cache read 0 · cache write 900`; a figure the provider never
/// reports reads `n/a`.
fn usage_text(u: &Usage) -> String {
    format!(
        "in {} · out {} · cache read {} · cache write {}",
        u.input_tokens,
        u.output_tokens,
        reported(u.cache_read_input_tokens),
        reported(u.cache_creation_input_tokens)
    )
}

fn reported(value: Option<u64>) -> String {
    value.map_or_else(|| "n/a".to_string(), |n| n.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_cache_read_is_shown_as_a_zero_and_an_unreported_one_as_na() {
        let claude = Usage {
            input_tokens: 1200,
            output_tokens: 80,
            cache_creation_input_tokens: Some(900),
            cache_read_input_tokens: Some(0),
        };
        assert_eq!(
            usage_text(&claude),
            "in 1200 · out 80 · cache read 0 · cache write 900"
        );
        let ollama = Usage {
            input_tokens: 5,
            output_tokens: 2,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
        };
        assert_eq!(
            usage_text(&ollama),
            "in 5 · out 2 · cache read n/a · cache write n/a"
        );
    }
}
