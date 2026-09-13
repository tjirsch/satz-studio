//! The agent loop over a scripted provider and a mock tool host: tool calls, the
//! approval gate, the ends of a turn, and what the transcript looks like after each.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use satz_studio_core::llm::agent::PREAMBLE;
use satz_studio_core::llm::{
    Agent, AgentEvent, Approval, Capabilities, ChatProvider, ClaudeError, ContentBlock, Effort, EstateContext, Message, Request, Role, StopDetails, StopReason, StreamEvent, StreamFuture, ToolHost,
    Usage, tool_defs, tool_result,
};
use satz_studio_core::satz::{SatzError, ToolAnnotations, ToolInfo, ToolOutcome};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// One scripted request: the events to send, then either the end or a wait for
/// cancellation.
enum Script {
    Events(Vec<StreamEvent>),
    HangUntilCancelled(Vec<StreamEvent>),
}

struct MockProvider {
    scripts: Mutex<VecDeque<Script>>,
    requests: Mutex<Vec<Request>>,
}

impl MockProvider {
    fn new(scripts: Vec<Script>) -> Arc<Self> {
        Arc::new(Self { scripts: Mutex::new(scripts.into()), requests: Mutex::new(Vec::new()) })
    }
    fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
}

impl ChatProvider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { tools: true, thinking: true, effort: true, cache_control: true }
    }
    fn stream<'a>(&'a self, req: &'a Request, tx: mpsc::Sender<StreamEvent>, cancel: CancellationToken) -> StreamFuture<'a> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(req.clone());
            let script = self.scripts.lock().unwrap().pop_front().expect("a scripted request");
            match script {
                Script::Events(events) => {
                    for event in events {
                        tx.send(event).await.expect("the agent listens");
                    }
                    Ok(())
                }
                Script::HangUntilCancelled(events) => {
                    for event in events {
                        tx.send(event).await.expect("the agent listens");
                    }
                    cancel.cancelled().await;
                    Err(ClaudeError::Cancelled)
                }
            }
        })
    }
}

struct MockHost {
    calls: Mutex<Vec<(String, serde_json::Map<String, serde_json::Value>)>>,
    /// a tool whose call fails below the tool
    broken: Option<String>,
}

impl MockHost {
    fn new() -> Arc<Self> {
        Arc::new(Self { calls: Mutex::new(Vec::new()), broken: None })
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().iter().map(|(n, _)| n.clone()).collect()
    }
}

fn info(name: &str, description: &str, read_only: bool, destructive: bool, output: Option<serde_json::Value>) -> ToolInfo {
    ToolInfo {
        name: name.to_string(),
        description: description.to_string(),
        input_schema: serde_json::json!({"type": "object", "properties": {"estate": {"type": "string"}}}),
        output_schema: output,
        annotations: ToolAnnotations { read_only: Some(read_only), destructive: Some(destructive), idempotent: None, open_world: None },
    }
}

impl ToolHost for MockHost {
    fn tools(&self) -> Vec<ToolInfo> {
        vec![
            info("satz_wipe", "Remove every generated file.", false, true, None),
            info("satz_questions", "The questions of the estate.", true, false, Some(serde_json::json!({"type": "object", "properties": {"open": {}, "answered": {}}}))),
            info("satz_interview", "Answer questions", false, false, None),
        ]
    }
    fn instructions(&self) -> String {
        "instructions from the server".to_string()
    }
    fn guide(&self) -> String {
        "the guide text".to_string()
    }
    fn call<'a>(&'a self, name: &'a str, args: serde_json::Map<String, serde_json::Value>) -> Pin<Box<dyn Future<Output = Result<ToolOutcome, SatzError>> + Send + 'a>> {
        Box::pin(async move {
            if self.broken.as_deref() == Some(name) {
                return Err(SatzError::Closed("exit status: 1".to_string()));
            }
            self.calls.lock().unwrap().push((name.to_string(), args));
            Ok(ToolOutcome { structured: Some(serde_json::json!({"ok": true, "tool": name})), text: String::new(), is_error: false })
        })
    }
}

/// What the collector saw, without the channel half of a pending card.
#[derive(Debug, PartialEq)]
enum Seen {
    Started,
    Text(String),
    Thinking(String),
    ToolUseStarted(String),
    ToolInputDelta(String),
    Pending(String),
    Result { name: String, is_error: bool, content: String },
    TurnDone(StopReason),
    Refused(Option<String>),
    Failed(String),
    Cancelled,
}

fn started() -> StreamEvent {
    StreamEvent::Started { id: "msg_01".to_string(), model: "claude-opus-5".to_string() }
}
fn done(stop_reason: StopReason) -> StreamEvent {
    StreamEvent::Done { stop_reason, stop_details: None, usage: Usage { input_tokens: 10, output_tokens: 5, cache_creation_input_tokens: None, cache_read_input_tokens: Some(4) } }
}
fn text_turn(text: &str, stop: StopReason) -> Script {
    Script::Events(vec![started(), StreamEvent::TextDelta(text.to_string()), StreamEvent::BlockStop { index: 0, block: ContentBlock::text(text) }, done(stop)])
}
fn tool_call(index: usize, id: &str, name: &str, input: serde_json::Value) -> Vec<StreamEvent> {
    vec![
        StreamEvent::ToolUseStart { index, id: id.to_string(), name: name.to_string() },
        StreamEvent::ToolInputDelta { index, partial_json: input.to_string() },
        StreamEvent::BlockStop { index, block: ContentBlock::ToolUse { id: id.to_string(), name: name.to_string(), input } },
    ]
}
fn tool_turn(calls: Vec<(&str, &str, serde_json::Value)>) -> Script {
    let mut events = vec![started(), StreamEvent::TextDelta("Calling.".to_string()), StreamEvent::BlockStop { index: 0, block: ContentBlock::text("Calling.") }];
    for (i, (id, name, input)) in calls.into_iter().enumerate() {
        events.extend(tool_call(i + 1, id, name, input));
    }
    events.push(done(StopReason::ToolUse));
    Script::Events(events)
}

fn agent(provider: Arc<MockProvider>, host: Arc<MockHost>) -> Agent {
    Agent::with_host(provider, host, "claude-opus-5".to_string(), Effort::High)
}

/// Run one turn with a collector that answers every card with `policy` and cancels
/// on the first text delta when `cancel_on_text`.
async fn drive(agent: &mut Agent, text: &str, policy: Approval, cancel_on_text: bool) -> (Result<(), ClaudeError>, Vec<Seen>) {
    let (tx, mut rx) = mpsc::channel(16);
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let collector = tokio::spawn(async move {
        let mut seen = Vec::new();
        while let Some(event) = rx.recv().await {
            seen.push(match event {
                AgentEvent::Started => Seen::Started,
                AgentEvent::TextDelta(t) => {
                    if cancel_on_text {
                        token.cancel();
                    }
                    Seen::Text(t)
                }
                AgentEvent::ThinkingDelta(t) => Seen::Thinking(t),
                AgentEvent::ToolUseStarted { name, .. } => Seen::ToolUseStarted(name),
                AgentEvent::ToolInputDelta { partial_json, .. } => Seen::ToolInputDelta(partial_json),
                AgentEvent::ToolCallPending { name, approval, .. } => {
                    approval.send(policy).expect("the agent waits");
                    Seen::Pending(name)
                }
                AgentEvent::ToolResult { name, outcome, .. } => Seen::Result { name, is_error: outcome.is_error, content: outcome.structured.map(|v| v.to_string()).unwrap_or(outcome.text) },
                AgentEvent::TurnDone { stop_reason, .. } => Seen::TurnDone(stop_reason),
                AgentEvent::Refused { category, .. } => Seen::Refused(category),
                AgentEvent::Failed(e) => Seen::Failed(e.to_string()),
                AgentEvent::Cancelled => Seen::Cancelled,
            });
        }
        seen
    });
    let result = agent.run_turn(text.to_string(), tx, cancel).await;
    (result, collector.await.expect("the collector ends"))
}

fn assert_send<T: Send>(_: &T) {}

#[tokio::test]
async fn a_tool_turn_returns_every_result_in_one_user_message_then_ends() {
    let provider = MockProvider::new(vec![
        tool_turn(vec![("toolu_1", "satz_questions", serde_json::json!({})), ("toolu_2", "satz_questions", serde_json::json!({"estate": "acme"}))]),
        text_turn("Two answers.", StopReason::EndTurn),
    ]);
    let host = MockHost::new();
    let mut agent = agent(provider.clone(), host.clone());
    let future = drive(&mut agent, "what is open?", Approval::Deny, false);
    assert_send(&future);
    let (result, seen) = future.await;
    result.expect("the turn ends");

    assert_eq!(host.calls(), vec!["satz_questions", "satz_questions"]);
    assert_eq!(agent.messages.len(), 4);
    assert_eq!(agent.messages[0], Message::user(vec![ContentBlock::text("what is open?")]));
    assert_eq!(agent.messages[1].role, Role::Assistant);
    assert_eq!(agent.messages[1].content.len(), 3);
    let results = &agent.messages[2];
    assert_eq!(results.role, Role::User);
    assert_eq!(results.content.len(), 2, "both results in one user message");
    let expected = serde_json::to_string_pretty(&serde_json::json!({"ok": true, "tool": "satz_questions"})).unwrap();
    assert_eq!(results.content[0], ContentBlock::ToolResult { tool_use_id: "toolu_1".to_string(), content: expected.clone(), is_error: false, cache_control: None });
    assert_eq!(results.content[1], ContentBlock::ToolResult { tool_use_id: "toolu_2".to_string(), content: expected, is_error: false, cache_control: None });
    assert_eq!(agent.messages[3], Message::assistant(vec![ContentBlock::text("Two answers.")]));

    assert!(!seen.contains(&Seen::Pending("satz_questions".to_string())), "a read-only tool asks nobody");
    assert_eq!(seen.iter().filter(|s| matches!(s, Seen::Result { is_error: false, .. })).count(), 2);
    assert_eq!(seen.last(), Some(&Seen::TurnDone(StopReason::EndTurn)));
    assert_eq!(seen.iter().filter(|s| **s == Seen::Started).count(), 2);

    let requests = provider.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].messages, agent.messages[..3].to_vec(), "the second request carries the transcript so far");
    assert!(requests[0].system[0].text.starts_with(PREAMBLE));
    assert!(requests[0].system[0].text.contains("instructions from the server"));
    assert!(requests[0].system[0].text.ends_with("the guide text"));
    assert!(requests[0].system[0].cache_control.is_some());
    assert_eq!(requests[0].system.len(), 1, "no context was set");
    assert_eq!(requests[0].tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec!["satz_interview", "satz_questions", "satz_wipe"]);
    assert_eq!(requests[0].model, "claude-opus-5");
    assert!(requests[0].fallbacks);
}

#[tokio::test]
async fn a_write_tool_waits_for_approval_and_a_deny_is_an_error_result() {
    let provider = MockProvider::new(vec![tool_turn(vec![("toolu_1", "satz_interview", serde_json::json!({"estate": "acme"}))]), text_turn("Understood.", StopReason::EndTurn)]);
    let host = MockHost::new();
    let mut agent = agent(provider, host.clone());
    let (result, seen) = drive(&mut agent, "answer them", Approval::Deny, false).await;
    result.expect("the loop continues after a deny");
    assert!(host.calls().is_empty(), "a denied tool never ran");
    assert!(seen.contains(&Seen::Pending("satz_interview".to_string())));
    assert!(seen.contains(&Seen::Result { name: "satz_interview".to_string(), is_error: true, content: "denied by the operator".to_string() }));
    assert_eq!(agent.messages[2].content[0], ContentBlock::ToolResult { tool_use_id: "toolu_1".to_string(), content: "denied by the operator".to_string(), is_error: true, cache_control: None });
    assert_eq!(agent.messages.len(), 4);
}

#[tokio::test]
async fn for_session_skips_the_card_the_second_time() {
    let write = || tool_turn(vec![("toolu_1", "satz_interview", serde_json::json!({}))]);
    let provider = MockProvider::new(vec![write(), write(), text_turn("Done.", StopReason::EndTurn)]);
    let host = MockHost::new();
    let mut agent = agent(provider, host.clone());
    let (result, seen) = drive(&mut agent, "answer twice", Approval::ForSession, false).await;
    result.expect("ends");
    assert_eq!(seen.iter().filter(|s| matches!(s, Seen::Pending(_))).count(), 1, "one card for the session");
    assert_eq!(host.calls(), vec!["satz_interview", "satz_interview"]);
    assert!(agent.allowed.contains_key("satz_interview"));
}

#[tokio::test]
async fn once_asks_every_time() {
    let write = || tool_turn(vec![("toolu_1", "satz_interview", serde_json::json!({}))]);
    let provider = MockProvider::new(vec![write(), write(), text_turn("Done.", StopReason::EndTurn)]);
    let host = MockHost::new();
    let mut agent = agent(provider, host.clone());
    let (result, seen) = drive(&mut agent, "answer twice", Approval::Once, false).await;
    result.expect("ends");
    assert_eq!(seen.iter().filter(|s| matches!(s, Seen::Pending(_))).count(), 2);
    assert!(agent.allowed.is_empty());
}

#[tokio::test]
async fn auto_approved_writes_skip_the_card_but_a_destructive_tool_asks() {
    let provider =
        MockProvider::new(vec![tool_turn(vec![("toolu_1", "satz_interview", serde_json::json!({})), ("toolu_2", "satz_wipe", serde_json::json!({}))]), text_turn("Done.", StopReason::EndTurn)]);
    let host = MockHost::new();
    let mut agent = agent(provider, host.clone());
    agent.auto_approve_writes = true;
    let (result, seen) = drive(&mut agent, "go", Approval::Once, false).await;
    result.expect("ends");
    assert_eq!(seen.iter().filter_map(|s| if let Seen::Pending(n) = s { Some(n.as_str()) } else { None }).collect::<Vec<_>>(), vec!["satz_wipe"]);
    assert_eq!(host.calls(), vec!["satz_interview", "satz_wipe"]);
}

#[tokio::test]
async fn max_tokens_ends_the_turn_and_keeps_the_message() {
    let provider = MockProvider::new(vec![text_turn("cut", StopReason::MaxTokens)]);
    let mut agent = agent(provider, MockHost::new());
    let (result, seen) = drive(&mut agent, "write a lot", Approval::Deny, false).await;
    result.expect("a cut turn still ends");
    assert_eq!(seen.last(), Some(&Seen::TurnDone(StopReason::MaxTokens)));
    assert_eq!(agent.messages.len(), 2);
}

#[tokio::test]
async fn a_refusal_discards_the_turn() {
    let details = StopDetails { kind: "refusal".to_string(), category: Some("cyber".to_string()), explanation: Some("no".to_string()), recommended_model: Some("claude-opus-4-8".to_string()) };
    let provider = MockProvider::new(vec![Script::Events(vec![
        started(),
        StreamEvent::TextDelta("I can".to_string()),
        StreamEvent::BlockStop { index: 0, block: ContentBlock::text("I can") },
        StreamEvent::Done { stop_reason: StopReason::Refusal, stop_details: Some(details), usage: Usage::default() },
    ])]);
    let mut agent = agent(provider, MockHost::new());
    agent.messages = vec![Message::user(vec![ContentBlock::text("earlier")]), Message::assistant(vec![ContentBlock::text("fine")])];
    let before = agent.messages.clone();
    let (result, seen) = drive(&mut agent, "exploit this", Approval::Deny, false).await;
    assert!(matches!(result, Err(ClaudeError::Refused { category: Some(c), recommended_model: Some(m), .. }) if c == "cyber" && m == "claude-opus-4-8"));
    assert_eq!(agent.messages, before, "the assistant message and the user message of the turn are gone");
    assert_eq!(seen.last(), Some(&Seen::Refused(Some("cyber".to_string()))));
}

#[tokio::test]
async fn cancellation_mid_stream_leaves_messages_as_before_the_turn() {
    let provider = MockProvider::new(vec![Script::HangUntilCancelled(vec![started(), StreamEvent::TextDelta("Hel".to_string())])]);
    let mut agent = agent(provider, MockHost::new());
    agent.messages = vec![Message::user(vec![ContentBlock::text("earlier")]), Message::assistant(vec![ContentBlock::text("fine")])];
    let before = agent.messages.clone();
    let (result, seen) = drive(&mut agent, "go on", Approval::Deny, true).await;
    assert!(matches!(result, Err(ClaudeError::Cancelled)));
    assert_eq!(agent.messages, before);
    assert_eq!(seen, vec![Seen::Started, Seen::Text("Hel".to_string()), Seen::Cancelled]);
}

#[tokio::test]
async fn a_failure_below_a_tool_fails_and_discards_the_turn() {
    let provider = MockProvider::new(vec![tool_turn(vec![("toolu_1", "satz_questions", serde_json::json!({}))])]);
    let host = Arc::new(MockHost { calls: Mutex::new(Vec::new()), broken: Some("satz_questions".to_string()) });
    let mut agent = agent(provider, host);
    let (result, seen) = drive(&mut agent, "what is open?", Approval::Deny, false).await;
    assert!(matches!(&result, Err(ClaudeError::Tool { name, message }) if name == "satz_questions" && message.contains("closed")), "{result:?}");
    assert!(agent.messages.is_empty());
    assert!(matches!(seen.last(), Some(Seen::Failed(m)) if m.contains("satz_questions")));
}

#[tokio::test]
async fn a_tool_the_host_does_not_have_is_an_error_result() {
    let provider = MockProvider::new(vec![tool_turn(vec![("toolu_1", "satz_imagined", serde_json::json!({}))]), text_turn("Noted.", StopReason::EndTurn)]);
    let mut agent = agent(provider, MockHost::new());
    let (result, seen) = drive(&mut agent, "go", Approval::Once, false).await;
    result.expect("the model is told and continues");
    assert!(seen.contains(&Seen::Result { name: "satz_imagined".to_string(), is_error: true, content: "no such tool: satz_imagined".to_string() }));
}

#[tokio::test]
async fn pause_turn_continues_without_a_new_user_message() {
    let provider = MockProvider::new(vec![text_turn("half", StopReason::PauseTurn), text_turn("rest", StopReason::EndTurn)]);
    let mut agent = agent(provider.clone(), MockHost::new());
    let (result, _) = drive(&mut agent, "go", Approval::Deny, false).await;
    result.expect("ends");
    let requests = provider.requests();
    assert_eq!(requests[1].messages.len(), 2, "the paused assistant message is sent back as is");
    assert_eq!(requests[1].messages[1].role, Role::Assistant);
    assert_eq!(agent.messages.len(), 3);
}

#[tokio::test]
async fn the_estate_context_is_the_second_system_block_without_a_breakpoint() {
    let mut agent = agent(MockProvider::new(vec![]), MockHost::new());
    agent.set_context(EstateContext {
        path: "/estates/acme".to_string(),
        runs_as: Some("iac@example.com".to_string()),
        deployment_mode: Some("org".to_string()),
        questions_summary: Some("3 open".to_string()),
        diagnostics: (0..25).map(|i| format!("warning {i}")).collect(),
        outline: vec!["folder \"platform\"".to_string()],
    });
    let request = agent.request();
    assert_eq!(request.system.len(), 2);
    assert!(request.system[1].cache_control.is_none());
    let text = &request.system[1].text;
    assert!(text.starts_with("estate: /estates/acme\nruns as: iac@example.com\ndeployment mode: org\nquestions: 3 open\ndiagnostics (25):\n  warning 0\n"));
    assert!(text.contains("  warning 19\n  … and 5 more\noutline:\n  folder \"platform\""));
    assert!(!text.contains("warning 20"));
}

#[test]
fn tool_defs_describe_the_output_keys_sort_by_name_and_mark_the_last() {
    let defs = tool_defs(&MockHost { calls: Mutex::new(Vec::new()), broken: None }.tools());
    assert_eq!(defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), vec!["satz_interview", "satz_questions", "satz_wipe"]);
    assert_eq!(defs[0].description, "Answer questions");
    assert_eq!(defs[1].description, "The questions of the estate. Returns: {answered, open}");
    assert_eq!(defs[1].input_schema, serde_json::json!({"type": "object", "properties": {"estate": {"type": "string"}}}));
    assert!(defs[0].cache_control.is_none() && defs[1].cache_control.is_none());
    assert!(defs[2].cache_control.is_some());
    assert!(tool_defs(&[]).is_empty());
}

#[test]
fn tool_result_prefers_the_structured_payload_and_carries_is_error() {
    let structured = ToolOutcome { structured: Some(serde_json::json!({"b": 1, "a": [true]})), text: "ignored".to_string(), is_error: false };
    assert_eq!(
        tool_result("toolu_1", &structured),
        ContentBlock::ToolResult { tool_use_id: "toolu_1".to_string(), content: "{\n  \"a\": [\n    true\n  ],\n  \"b\": 1\n}".to_string(), is_error: false, cache_control: None }
    );
    let refused = ToolOutcome { structured: None, text: "satz_apply is not served".to_string(), is_error: true };
    assert_eq!(tool_result("toolu_2", &refused), ContentBlock::ToolResult { tool_use_id: "toolu_2".to_string(), content: "satz_apply is not served".to_string(), is_error: true, cache_control: None });
}
