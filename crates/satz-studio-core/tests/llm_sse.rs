//! The SSE decoder and the assembler over hand-written recordings under
//! `fixtures/sse/`: each is fed as a byte stream in small chunks and as one piece.

use satz_studio_core::llm::claude::sse::{Assembler, SseDecoder, SseEvent};
use satz_studio_core::llm::{ClaudeError, ContentBlock, Response, StopReason, StreamEvent, Usage};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/sse/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("the recording exists")
}

/// Decode and assemble a body fed in `chunk`-byte pieces.
fn run(bytes: &[u8], chunk: usize) -> Result<(Vec<StreamEvent>, Response), ClaudeError> {
    let mut decoder = SseDecoder::new();
    let mut assembler = Assembler::new();
    let mut events = Vec::new();
    for piece in bytes.chunks(chunk) {
        for event in decoder.push(piece)? {
            events.extend(assembler.feed(&event)?);
        }
    }
    decoder.finish()?;
    let response = assembler.finish()?;
    Ok((events, response))
}

fn text(text: &str) -> ContentBlock {
    ContentBlock::Text {
        text: text.to_string(),
        cache_control: None,
    }
}

#[test]
fn a_text_turn_assembles_the_same_in_small_chunks_and_whole() {
    let bytes = fixture("text_turn.sse");
    let (events, response) = run(&bytes, 7).expect("assembles");
    let (whole_events, whole_response) = run(&bytes, bytes.len()).expect("assembles");
    assert_eq!(events, whole_events);
    assert_eq!(response, whole_response);

    let thinking = ContentBlock::Thinking {
        thinking: "The estate has one folder.".to_string(),
        signature: "EqQBCgIYAhIM".to_string(),
    };
    let usage = Usage {
        input_tokens: 25,
        output_tokens: 15,
        cache_creation_input_tokens: Some(0),
        cache_read_input_tokens: Some(0),
    };
    assert_eq!(
        events,
        vec![
            StreamEvent::Started {
                id: "msg_01TextTurnExample".to_string(),
                model: "claude-opus-5".to_string()
            },
            StreamEvent::ThinkingDelta("The estate has one folder.".to_string()),
            StreamEvent::BlockStop {
                index: 0,
                block: thinking.clone()
            },
            StreamEvent::TextDelta("Hello".to_string()),
            StreamEvent::TextDelta(", operator.".to_string()),
            StreamEvent::BlockStop {
                index: 1,
                block: text("Hello, operator.")
            },
            StreamEvent::Done {
                stop_reason: StopReason::EndTurn,
                stop_details: None,
                usage
            },
        ]
    );
    assert_eq!(
        response,
        Response {
            id: "msg_01TextTurnExample".to_string(),
            model: "claude-opus-5".to_string(),
            content: vec![thinking, text("Hello, operator.")],
            stop_reason: StopReason::EndTurn,
            stop_details: None,
            usage
        }
    );
}

#[test]
fn a_tool_turn_concatenates_the_input_json_deltas_and_parses_at_block_stop() {
    let (events, response) = run(&fixture("tool_turn_split_json.sse"), 5).expect("assembles");
    let input = serde_json::json!({"estate": "estates/acme.satz"});
    let tool_use = ContentBlock::ToolUse {
        id: "toolu_01CheckExample".to_string(),
        name: "satz_transpile_check".to_string(),
        input: input.clone(),
    };
    assert_eq!(
        events,
        vec![
            StreamEvent::Started {
                id: "msg_01ToolTurnExample".to_string(),
                model: "claude-opus-5".to_string()
            },
            StreamEvent::TextDelta("Checking the estate.".to_string()),
            StreamEvent::BlockStop {
                index: 0,
                block: text("Checking the estate.")
            },
            StreamEvent::ToolUseStart {
                index: 1,
                id: "toolu_01CheckExample".to_string(),
                name: "satz_transpile_check".to_string()
            },
            StreamEvent::ToolInputDelta {
                index: 1,
                partial_json: "{\"est".to_string()
            },
            StreamEvent::ToolInputDelta {
                index: 1,
                partial_json: "ate\": \"estates/ac".to_string()
            },
            StreamEvent::ToolInputDelta {
                index: 1,
                partial_json: "me.satz\"}".to_string()
            },
            StreamEvent::BlockStop {
                index: 1,
                block: tool_use.clone()
            },
            StreamEvent::Done {
                stop_reason: StopReason::ToolUse,
                stop_details: None,
                usage: Usage {
                    input_tokens: 412,
                    output_tokens: 61,
                    cache_creation_input_tokens: Some(380),
                    cache_read_input_tokens: Some(0)
                },
            },
        ]
    );
    assert_eq!(response.stop_reason, StopReason::ToolUse);
    assert_eq!(response.content[1], tool_use);
    let ContentBlock::ToolUse { input: parsed, .. } = &response.content[1] else {
        panic!("a tool_use block")
    };
    assert_eq!(parsed["estate"], "estates/acme.satz");
}

#[test]
fn a_refusal_carries_its_stop_details() {
    let (events, response) = run(&fixture("refusal.sse"), 11).expect("assembles");
    assert_eq!(response.stop_reason, StopReason::Refusal);
    let details = response
        .stop_details
        .clone()
        .expect("stop_details on a refusal");
    assert_eq!(details.kind, "refusal");
    assert_eq!(details.category.as_deref(), Some("cyber"));
    assert_eq!(
        details.explanation.as_deref(),
        Some("The request asks for a working exploit.")
    );
    assert_eq!(
        details.recommended_model.as_deref(),
        Some("claude-opus-4-8")
    );
    assert!(
        matches!(events.last(), Some(StreamEvent::Done { stop_reason: StopReason::Refusal, stop_details: Some(d), .. }) if d.category.as_deref() == Some("cyber"))
    );
    assert_eq!(response.content, vec![text("I can")]);
}

#[test]
fn a_mid_stream_error_event_fails_the_stream_after_what_arrived() {
    let bytes = fixture("mid_stream_error.sse");
    let mut decoder = SseDecoder::new();
    let mut assembler = Assembler::new();
    let mut events = Vec::new();
    let mut failure = None;
    'outer: for piece in bytes.chunks(9) {
        for event in decoder.push(piece).expect("decodes") {
            match assembler.feed(&event) {
                Ok(more) => events.extend(more),
                Err(e) => {
                    failure = Some(e);
                    break 'outer;
                }
            }
        }
    }
    let Some(ClaudeError::Stream(message)) = failure else {
        panic!("a stream error, got {failure:?}")
    };
    assert_eq!(message, "overloaded_error: Overloaded");
    assert_eq!(
        events,
        vec![
            StreamEvent::Started {
                id: "msg_01ErrorExample".to_string(),
                model: "claude-opus-5".to_string()
            },
            StreamEvent::TextDelta("Looking".to_string())
        ]
    );
    assert!(assembler.started());
}

#[test]
fn a_fallback_block_is_kept_verbatim_and_round_trips() {
    let (events, response) = run(&fixture("fallback_block.sse"), 13).expect("assembles");
    let raw = serde_json::json!({"type": "fallback", "from": {"type": "model", "model": "claude-opus-5"}, "to": {"type": "model", "model": "claude-opus-4-8"}});
    assert_eq!(response.model, "claude-opus-4-8");
    assert_eq!(response.content[0], ContentBlock::Other(raw.clone()));
    assert_eq!(response.content[0].kind(), "fallback");
    assert_eq!(
        events[1],
        StreamEvent::BlockStop {
            index: 0,
            block: ContentBlock::Other(raw.clone())
        }
    );
    // replayed verbatim: serialising the block is the JSON the API sent, and reading it back is the same block
    assert_eq!(
        serde_json::to_value(&response.content[0]).expect("serialises"),
        raw
    );
    let back: ContentBlock = serde_json::from_value(raw.clone()).expect("deserialises");
    assert_eq!(back, ContentBlock::Other(raw));
    assert_eq!(response.content[1], text("Done on the fallback model."));
}

#[test]
fn the_decoder_joins_data_lines_accepts_crlf_and_ignores_comments_and_ids() {
    let mut decoder = SseDecoder::new();
    let events = decoder.push(b": a comment\r\nid: 7\r\nevent: ping\r\ndata: {\"a\":\r\ndata:  1}\r\n\r\ndata: solo\n\n").expect("decodes");
    assert_eq!(
        events,
        vec![
            SseEvent {
                event: "ping".to_string(),
                data: "{\"a\":\n 1}".to_string()
            },
            SseEvent {
                event: "message".to_string(),
                data: "solo".to_string()
            }
        ]
    );
    decoder.finish().expect("nothing pending");
}

#[test]
fn a_stream_cut_inside_an_event_is_an_error() {
    let mut decoder = SseDecoder::new();
    assert!(
        decoder
            .push(b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n")
            .expect("decodes")
            .is_empty()
    );
    assert!(
        matches!(decoder.finish(), Err(ClaudeError::Stream(m)) if m.contains("inside an event"))
    );
    let mut decoder = SseDecoder::new();
    decoder.push(b"data: {}\n\ndata: {").expect("decodes");
    assert!(matches!(decoder.finish(), Err(ClaudeError::Stream(_))));
}

#[test]
fn a_stream_without_message_stop_is_an_error() {
    let bytes = fixture("text_turn.sse");
    let cut = &bytes[..bytes.len() - 42];
    assert!(
        matches!(run(cut, 64), Err(ClaudeError::Stream(m)) if m.contains("message_stop") || m.contains("inside an event"))
    );
}

#[test]
fn a_delta_the_code_does_not_fold_is_an_error() {
    let mut assembler = Assembler::new();
    let start = SseEvent {
        event: "content_block_start".to_string(),
        data:
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#
                .to_string(),
    };
    assembler.feed(&start).expect("a start");
    let delta = SseEvent { event: "content_block_delta".to_string(), data: r#"{"type":"content_block_delta","index":0,"delta":{"type":"citations_delta","citation":{}}}"#.to_string() };
    assert!(
        matches!(assembler.feed(&delta), Err(ClaudeError::Stream(m)) if m.contains("citations_delta"))
    );
    let out_of_order = SseEvent {
        event: "content_block_start".to_string(),
        data:
            r#"{"type":"content_block_start","index":5,"content_block":{"type":"text","text":""}}"#
                .to_string(),
    };
    assert!(
        matches!(assembler.feed(&out_of_order), Err(ClaudeError::Stream(m)) if m.contains("content block 5"))
    );
}

#[test]
fn an_event_the_code_does_not_read_is_an_error() {
    let mut assembler = Assembler::new();
    let event = SseEvent {
        event: "message_compaction".to_string(),
        data: r#"{"type":"message_compaction"}"#.to_string(),
    };
    assert!(
        matches!(assembler.feed(&event), Err(ClaudeError::Stream(m)) if m.contains("message_compaction"))
    );
}
