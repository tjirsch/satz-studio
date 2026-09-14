//! The rail of the open estate's transcripts, newest first, and "New". On the Claude
//! Code engine there are none to list: that engine holds its conversation in its own
//! process, and "New" starts a fresh one (ADR 0010).

use std::path::Path;

use dioxus::prelude::*;

use super::actions::ChatAction;
use super::state::{ChatStore, ChatStoreStoreExt};
use crate::components::{Button, ButtonVariant, Icon, List, ListItem};

#[component]
pub fn TranscriptRail() -> Element {
    let chat = use_context::<Store<ChatStore>>();
    let handle = use_coroutine_handle::<ChatAction>();
    let current = chat.transcript().cloned();
    let claude_code = chat.engine().cloned().is_claude_code();
    let transcripts = if claude_code {
        Vec::new()
    } else {
        chat.transcripts().cloned()
    };
    let busy = chat.busy().cloned();
    rsx! {
        aside { class: "chat__rail", class: if busy { "chat__rail--locked" }, "aria-label": "transcripts",
            div { class: "chat__rail-header",
                h2 { class: "chat__rail-title", "Transcripts" }
                Button { variant: ButtonVariant::Tonal, icon: "add", disabled: busy, onclick: move |_| handle.send(ChatAction::NewTranscript), "New" }
            }
            if claude_code {
                p { class: "chat__rail-empty", "Claude Code keeps this conversation itself. New starts a fresh session." }
            } else if transcripts.is_empty() {
                p { class: "chat__rail-empty", "None kept for this estate yet." }
            }
            List { class: "chat__rail-list",
                for path in transcripts {
                    {
                        let selected = current.as_ref() == Some(&path);
                        let target = path.clone();
                        rsx! {
                            ListItem {
                                key: "{path.display()}",
                                headline: transcript_label(&path),
                                selected,
                                leading: rsx! { Icon { name: "forum", size: 20, filled: selected } },
                                onclick: move |_| handle.send(ChatAction::Resume(target.clone())),
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The instant a transcript was started, read from its file name
/// (`2026-09-13T21-56-00.123456Z.jsonl` is `2026-09-13 21:56:00 UTC`); a name of
/// another shape is shown as it is.
pub fn transcript_label(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let b = stem.as_bytes();
    let shaped = b.len() >= 20
        && b[..19].iter().enumerate().all(|(i, c)| match i {
            4 | 7 | 13 | 16 => *c == b'-',
            10 => *c == b'T',
            _ => c.is_ascii_digit(),
        })
        && b[19] == b'.'
        && stem.ends_with('Z');
    if shaped {
        format!(
            "{} {}:{}:{} UTC",
            &stem[..10],
            &stem[11..13],
            &stem[14..16],
            &stem[17..19]
        )
    } else {
        stem
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_reads_the_instant_from_the_file_name() {
        assert_eq!(
            transcript_label(Path::new("/t/2026-09-13T21-56-00.123456Z.jsonl")),
            "2026-09-13 21:56:00 UTC"
        );
        assert_eq!(transcript_label(Path::new("/t/notes.jsonl")), "notes");
        assert_eq!(
            transcript_label(Path::new("/t/2026-09-13T21-56-00.jsonl")),
            "2026-09-13T21-56-00"
        );
    }
}
