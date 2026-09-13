//! One real request against the API, opt-in: `SATZ_STUDIO_LIVE=1` with a credential
//! that `Credential::resolve` finds. Without it the test prints a note and passes.

use std::future::Future;
use std::pin::Pin;

use satz_studio_core::llm::agent::PREAMBLE;
use satz_studio_core::llm::{
    CacheControl, ChatProvider, ClaudeClient, ContentBlock, Credential, Effort, Message, Request,
    Role, StreamEvent, SystemBlock, ToolHost, tool_defs,
};
use satz_studio_core::satz::{SatzError, ToolAnnotations, ToolInfo, ToolOutcome};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct MockHost;

impl ToolHost for MockHost {
    fn tools(&self) -> Vec<ToolInfo> {
        vec![
            ToolInfo {
                name: "satz_transpile_check".to_string(),
                description: "Check an estate without writing anything.".to_string(),
                input_schema: serde_json::json!({"type": "object", "properties": {"estate": {"type": "string"}}, "required": ["estate"]}),
                output_schema: Some(
                    serde_json::json!({"type": "object", "properties": {"ok": {"type": "boolean"}, "diagnostics": {"type": "array"}}}),
                ),
                annotations: ToolAnnotations {
                    read_only: Some(true),
                    destructive: Some(false),
                    idempotent: Some(true),
                    open_world: Some(false),
                },
            },
            ToolInfo {
                name: "satz_questions".to_string(),
                description: "The questions the estate's packs ask.".to_string(),
                input_schema: serde_json::json!({"type": "object", "properties": {}}),
                output_schema: None,
                annotations: ToolAnnotations {
                    read_only: Some(true),
                    destructive: Some(false),
                    idempotent: Some(true),
                    open_world: Some(false),
                },
            },
        ]
    }
    fn instructions(&self) -> String {
        "The tools read an example estate; none of them writes.".to_string()
    }
    fn guide(&self) -> String {
        // long enough to pass the model's minimum cacheable prefix
        (1..=400)
            .map(|i| format!("Rule {i}: an estate declares folder {i} once; the compiler refuses a second declaration of the same address, and a suppression that matches nothing is an error.\n"))
            .collect()
    }
    fn call<'a>(
        &'a self,
        _name: &'a str,
        _args: serde_json::Map<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<ToolOutcome, SatzError>> + Send + 'a>> {
        Box::pin(async {
            Ok(ToolOutcome {
                structured: None,
                text: "not called live".to_string(),
                is_error: true,
            })
        })
    }
}

async fn run(client: &ClaudeClient, req: &Request) -> Vec<StreamEvent> {
    let (tx, mut rx) = mpsc::channel(64);
    let (result, events) = tokio::join!(client.stream(req, tx, CancellationToken::new()), async {
        let mut events = Vec::new();
        while let Some(e) = rx.recv().await {
            events.push(e);
        }
        events
    });
    result.expect("the live request streams");
    events
}

#[tokio::test]
async fn a_live_request_streams_and_the_second_reads_the_cache() {
    if std::env::var("SATZ_STUDIO_LIVE").as_deref() != Ok("1") {
        println!("SATZ_STUDIO_LIVE is not 1: the live check is skipped");
        return;
    }
    let (credential, source) = Credential::resolve().await.expect("a credential");
    println!("credential from {source:?}");
    let client = ClaudeClient::new(credential);
    let host = MockHost;
    let req = Request {
        model: "claude-opus-5".to_string(),
        max_tokens: 64_000,
        system: vec![SystemBlock {
            text: format!("{PREAMBLE}\n\n{}\n\n{}", host.instructions(), host.guide()),
            cache_control: Some(CacheControl::ephemeral()),
        }],
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::text(
                "Reply with the single word: ready. Call no tool.",
            )],
        }],
        tools: tool_defs(&host.tools()),
        effort: Effort::Low,
        fallbacks: true,
    };

    let first = run(&client, &req).await;
    assert!(
        matches!(first.first(), Some(StreamEvent::Started { .. })),
        "{first:?}"
    );
    let Some(StreamEvent::Done { usage, .. }) = first.last() else {
        panic!("no Done: {first:?}")
    };
    println!("first: {usage:?}");

    let second = run(&client, &req).await;
    let Some(StreamEvent::Done { usage, .. }) = second.last() else {
        panic!("no Done: {second:?}")
    };
    println!("second: {usage:?}");
    assert!(
        usage.cache_read_input_tokens.unwrap_or(0) > 0,
        "the second identical request reads the cache: {usage:?}"
    );
}
