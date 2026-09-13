//! The transcript store on a temporary root: create, append, load, list, and a line
//! that is not a message.

use std::path::Path;

use satz_studio_core::llm::{ContentBlock, Message, Role};
use satz_studio_core::transcript::{TranscriptError, TranscriptStore};

fn store() -> (tempfile::TempDir, TranscriptStore) {
    let dir = tempfile::tempdir().expect("a temp dir");
    let store = TranscriptStore {
        root: dir.path().join("transcripts"),
    };
    (dir, store)
}

fn messages() -> Vec<Message> {
    vec![
        Message {
            role: Role::User,
            content: vec![ContentBlock::text("what is open?")],
        },
        Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Thinking {
                    thinking: "three".to_string(),
                    signature: "sig".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "toolu_01".to_string(),
                    name: "satz_questions".to_string(),
                    input: serde_json::json!({"estate": "estates/acme.satz"}),
                },
                ContentBlock::Other(
                    serde_json::json!({"type": "fallback", "from": {"model": "claude-opus-5"}, "to": {"model": "claude-opus-4-8"}}),
                ),
            ],
        },
    ]
}

#[test]
fn create_append_load_round_trips() {
    let (_dir, store) = store();
    let estate = Path::new("/estates/acme");
    let mut transcript = store.create(estate, "claude-opus-5").expect("creates");
    assert!(transcript.path.starts_with(store.dir_for(estate)));
    assert_eq!(
        transcript.path.extension().and_then(|x| x.to_str()),
        Some("jsonl")
    );
    assert!(
        !transcript
            .path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains(':')
    );
    assert_eq!(transcript.header.model, "claude-opus-5");
    assert_eq!(transcript.header.estate, estate);
    let created = &transcript.header.created;
    assert_eq!(created.len(), 27, "{created}");
    assert!(
        created.ends_with('Z') && &created[10..11] == "T" && &created[19..20] == ".",
        "{created}"
    );

    for message in messages() {
        store.append(&mut transcript, message).expect("appends");
    }
    assert_eq!(transcript.messages, messages());
    let loaded = store.load(&transcript.path).expect("loads");
    assert_eq!(loaded, transcript);
    assert_eq!(
        std::fs::read_to_string(&transcript.path)
            .unwrap()
            .lines()
            .count(),
        3,
        "the header and one line per message"
    );
}

#[test]
fn list_is_newest_first_and_empty_without_a_directory() {
    let (_dir, store) = store();
    let estate = Path::new("/estates/acme");
    assert!(
        store
            .list(estate)
            .expect("no directory is no transcripts")
            .is_empty()
    );
    let first = store.create(estate, "claude-opus-5").expect("creates");
    let second = store.create(estate, "claude-opus-5").expect("creates");
    assert_ne!(first.path, second.path);
    std::fs::write(store.dir_for(estate).join("notes.txt"), "not a transcript").unwrap();
    assert_eq!(
        store.list(estate).expect("lists"),
        vec![second.path, first.path]
    );
    assert!(
        store
            .list(Path::new("/estates/other"))
            .expect("lists")
            .is_empty()
    );
}

#[test]
fn a_line_that_is_not_a_message_is_reported_with_its_number() {
    let (_dir, store) = store();
    let mut transcript = store
        .create(Path::new("/estates/acme"), "claude-opus-5")
        .expect("creates");
    store
        .append(&mut transcript, messages().remove(0))
        .expect("appends");
    let mut text = std::fs::read_to_string(&transcript.path).unwrap();
    text.push_str("{\"role\": \"user\", \"content\": [{\"text\": \"no type\"}]}\n");
    std::fs::write(&transcript.path, text).unwrap();
    let error = store.load(&transcript.path).expect_err("a bad line");
    assert!(
        matches!(&error, TranscriptError::Line { line: 3, path, .. } if path == &transcript.path),
        "{error}"
    );

    std::fs::write(&transcript.path, "not json\n").unwrap();
    assert!(matches!(
        store.load(&transcript.path),
        Err(TranscriptError::Line { line: 1, .. })
    ));
    std::fs::write(&transcript.path, "").unwrap();
    assert!(matches!(
        store.load(&transcript.path),
        Err(TranscriptError::Io { .. })
    ));
    assert!(matches!(
        store.load(Path::new("/nowhere/none.jsonl")),
        Err(TranscriptError::Io { .. })
    ));
}
