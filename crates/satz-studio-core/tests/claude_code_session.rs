//! One Claude Code session over the smoke estate, driven by the fake CLI: the command
//! line the app builds, the events one turn translates into, the approval round trip,
//! and the interrupt. Offline — no claude.ai account and no network.

use std::sync::Arc;

use satz_studio_core::llm::claude_code::{
    ClaudeCodeError, Session, SessionOptions, allowed_tools, command_args, mcp_config,
    system_prompt,
};
use satz_studio_core::llm::{AgentEvent, Approval, StopReason};
use satz_studio_core::satz::{Allow, EstateSession};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[path = "fixtures/claude_code/support.rs"]
mod support;

use support::{Fake, SmokeCopy, copy_smoke, fake, fake_with, satz_binary, signed_in, within};

/// What the tests read back out of one turn: the events in order, flattened.
#[derive(Debug, Clone, PartialEq)]
enum Seen {
    Started,
    Text(String),
    ToolUseStarted(String),
    ToolInputDelta(String),
    Pending(String),
    Result {
        name: String,
        is_error: bool,
        body: String,
    },
    Notice(String),
    TurnDone(StopReason),
    Failed(String),
    Cancelled,
}

async fn options(auto_approve_writes: bool) -> SessionOptions {
    SessionOptions {
        model: None,
        max_turns: None,
        resume: None,
        auto_approve_writes,
        satz_binary: satz_binary().await,
        allow: Allow::ReadWrite,
    }
}

/// Run one turn, answering every approval card with `policy`; `cancel_on_text`
/// cancels as soon as the first text arrives.
async fn turn(
    session: &mut Session,
    text: &str,
    policy: Approval,
    cancel_on_text: bool,
) -> (Result<(), ClaudeCodeError>, Vec<Seen>) {
    let (tx, mut rx) = mpsc::channel(64);
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
                AgentEvent::ThinkingDelta(t) => Seen::Text(t),
                AgentEvent::ToolUseStarted { name, .. } => Seen::ToolUseStarted(name),
                AgentEvent::ToolInputDelta { partial_json, .. } => {
                    Seen::ToolInputDelta(partial_json)
                }
                AgentEvent::ToolCallPending { name, approval, .. } => {
                    approval.send(policy).expect("the session waits");
                    Seen::Pending(name)
                }
                AgentEvent::ToolResult { name, outcome, .. } => Seen::Result {
                    name,
                    is_error: outcome.is_error,
                    body: outcome
                        .structured
                        .map(|v| v.to_string())
                        .unwrap_or(outcome.text),
                },
                AgentEvent::Notice(text) => Seen::Notice(text),
                AgentEvent::TurnDone { stop_reason, .. } => Seen::TurnDone(stop_reason),
                AgentEvent::Refused { category, .. } => Seen::Failed(category.unwrap_or_default()),
                AgentEvent::Failed(e) => Seen::Failed(e.to_string()),
                AgentEvent::Cancelled => Seen::Cancelled,
            });
        }
        seen
    });
    let result = within(session.run_turn(text.to_string(), tx, cancel)).await;
    (result, collector.await.expect("the collector ends"))
}

/// The estate, the fake and a spawned session over both.
struct Running {
    _smoke: SmokeCopy,
    _tmp: tempfile::TempDir,
    estate: Arc<EstateSession>,
    fake: Fake,
    session: Session,
}

async fn start(scripts: &[&str], auto_approve_writes: bool) -> Running {
    start_with(scripts, auto_approve_writes, serde_json::Map::new()).await
}

async fn start_with(
    scripts: &[&str],
    auto_approve_writes: bool,
    extra: serde_json::Map<String, serde_json::Value>,
) -> Running {
    let smoke = copy_smoke();
    let estate = smoke.open().await;
    let tmp = tempfile::tempdir().unwrap();
    let fake = fake_with(tmp.path(), signed_in(), scripts, extra);
    let cli = fake.locate().await;
    let session = within(Session::spawn(
        &cli,
        Arc::clone(&estate),
        options(auto_approve_writes).await,
    ))
    .await
    .expect("the session spawns");
    Running {
        _smoke: smoke,
        _tmp: tmp,
        estate,
        fake,
        session,
    }
}

#[tokio::test]
async fn the_command_line_is_the_one_the_app_decided_on() {
    let smoke = copy_smoke();
    let estate = smoke.open().await;
    let opts = options(false).await;
    let prompt = system_prompt(&estate);
    let args = command_args(&estate, &opts, &prompt);

    let after = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .map(|i| args[i + 1].clone())
            .unwrap_or_else(|| panic!("no {flag} in {args:?}"))
    };
    // the flags that make this session the app's and not the user's
    assert_eq!(args[0], "-p");
    assert_eq!(after("--input-format"), "stream-json");
    assert_eq!(after("--output-format"), "stream-json");
    assert!(args.iter().any(|a| a == "--verbose"));
    assert!(args.iter().any(|a| a == "--include-partial-messages"));
    assert_eq!(after("--setting-sources"), "");
    assert!(args.iter().any(|a| a == "--strict-mcp-config"));
    assert_eq!(after("--tools"), "");
    assert_eq!(after("--permission-mode"), "default");
    assert_eq!(after("--permission-prompt-tool"), "stdio");
    assert!(!args.iter().any(|a| a == "--bare"), "{args:?}");
    // no model, no cap and no resume were asked for, so none is sent
    assert!(!args.iter().any(|a| a == "--model"), "{args:?}");
    assert!(!args.iter().any(|a| a == "--max-turns"), "{args:?}");
    assert!(!args.iter().any(|a| a == "--resume"), "{args:?}");

    // the estate's own satz MCP server, and nothing else
    let config: serde_json::Value = serde_json::from_str(&after("--mcp-config")).unwrap();
    assert_eq!(
        config,
        serde_json::from_str::<serde_json::Value>(&mcp_config(&estate, &opts)).unwrap()
    );
    let server = &config["mcpServers"]["satz"];
    assert_eq!(server["command"], opts.satz_binary.display().to_string());
    assert_eq!(server["args"][0], "mcp");
    assert_eq!(server["args"][1], "--root");
    assert_eq!(server["args"][3], "--allow");
    assert_eq!(server["args"][4], "read,write");

    // the system prompt is the studio preamble, the guide, and which estate this is
    let appended = after("--append-system-prompt");
    assert!(
        appended.starts_with("You are inside satz-studio"),
        "{appended}"
    );
    assert!(appended.contains("smoke.satz"), "{appended}");
    assert!(appended.contains("satz_open"), "{appended}");

    // read-only tools are pre-approved, a write is not
    let allowlist = after("--allowedTools");
    let allowed: Vec<&str> = allowlist.split(',').collect();
    assert!(
        allowed.contains(&"mcp__satz__satz_questions"),
        "{allowed:?}"
    );
    assert!(
        !allowed.contains(&"mcp__satz__satz_interview"),
        "{allowed:?}"
    );
    assert!(
        allowed.iter().all(|t| t.starts_with("mcp__satz__")),
        "{allowed:?}"
    );
}

#[tokio::test]
async fn auto_approving_writes_puts_them_on_the_allowlist() {
    let smoke = copy_smoke();
    let estate = smoke.open().await;
    let strict = allowed_tools(estate.tools(), false);
    let loose = allowed_tools(estate.tools(), true);
    assert!(strict.len() < loose.len(), "{strict:?} vs {loose:?}");
    assert!(!strict.contains(&"mcp__satz__satz_interview".to_string()));
    assert!(loose.contains(&"mcp__satz__satz_interview".to_string()));
    // a destructive tool never lands on the allowlist, whatever the setting
    let destructive: Vec<&str> = estate
        .tools()
        .iter()
        .filter(|t| t.annotations.destructive == Some(true))
        .map(|t| t.name.as_str())
        .collect();
    assert!(!destructive.is_empty(), "the estate has a destructive tool");
    for name in destructive {
        assert!(!loose.contains(&format!("mcp__satz__{name}")), "{name}");
    }
}

#[tokio::test]
async fn a_model_and_a_cap_reach_the_command_line() {
    let smoke = copy_smoke();
    let estate = smoke.open().await;
    let opts = SessionOptions {
        model: Some("opus".to_string()),
        max_turns: Some(4),
        resume: Some("s-1".to_string()),
        ..options(false).await
    };
    let args = command_args(&estate, &opts, "prompt");
    let after = |flag: &str| {
        let i = args.iter().position(|a| a == flag).expect(flag);
        args[i + 1].clone()
    };
    assert_eq!(after("--model"), "opus");
    assert_eq!(after("--max-turns"), "4");
    assert_eq!(after("--resume"), "s-1");
}

#[tokio::test]
async fn a_text_turn_streams_its_deltas_and_ends_with_the_usage() {
    let mut running = start(&["text-turn"], false).await;
    let (result, seen) = turn(
        &mut running.session,
        "which questions are open?",
        Approval::Deny,
        false,
    )
    .await;
    result.expect("the turn ends");
    assert_eq!(
        seen,
        vec![
            Seen::Started,
            Seen::Text("Two questions ".into()),
            Seen::Text("are open.".into()),
            Seen::TurnDone(StopReason::EndTurn),
        ]
    );
    // the session id and the model came from `system/init`
    assert_eq!(
        running.session.session_id(),
        Some("11111111-1111-1111-1111-111111111111")
    );
    assert_eq!(running.session.model(), Some("claude-opus-5"));

    // the app initialized the control protocol and sent the message as one user line
    let written = running.fake.stdin();
    assert_eq!(written[0]["request"]["subtype"], "initialize");
    assert_eq!(written[1]["type"], "user");
    assert_eq!(
        written[1]["message"]["content"],
        "which questions are open?"
    );

    // and the command line reached the CLI intact
    let args = running.fake.args();
    assert!(
        args.contains(&"--strict-mcp-config".to_string()),
        "{args:?}"
    );
    running.session.close().await;
}

#[tokio::test]
async fn a_write_tool_raises_the_card_and_allow_once_runs_it() {
    let mut running = start(&["tool-turn"], false).await;
    let (result, seen) = turn(
        &mut running.session,
        "answer the deployment mode",
        Approval::Once,
        false,
    )
    .await;
    result.expect("the turn ends");
    assert_eq!(
        seen,
        vec![
            Seen::Started,
            Seen::ToolUseStarted("satz_interview".into()),
            Seen::ToolInputDelta("{\"answers\"".into()),
            Seen::ToolInputDelta(": {\"deployment_mode\": \"cloud\"}}".into()),
            Seen::Pending("satz_interview".into()),
            Seen::Result {
                name: "satz_interview".into(),
                is_error: false,
                body: "{\"written\":true}".into(),
            },
            Seen::Started,
            Seen::Text("Answered.".into()),
            Seen::TurnDone(StopReason::EndTurn),
        ]
    );
    let answer = running
        .fake
        .stdin()
        .into_iter()
        .find(|l| l["type"] == "control_response")
        .expect("the app answered the request");
    assert_eq!(answer["response"]["request_id"], "cc-1");
    assert_eq!(answer["response"]["response"]["behavior"], "allow");
    assert_eq!(
        answer["response"]["response"]["updatedInput"],
        serde_json::json!({"answers": {"deployment_mode": "cloud"}})
    );
    running.session.close().await;
}

#[tokio::test]
async fn deny_answers_the_request_with_the_reason_and_the_turn_goes_on() {
    let mut running = start(&["tool-turn"], false).await;
    let (result, seen) = turn(&mut running.session, "answer it", Approval::Deny, false).await;
    result.expect("the turn ends");
    assert!(
        seen.contains(&Seen::Pending("satz_interview".into())),
        "{seen:?}"
    );
    assert!(
        seen.contains(&Seen::Result {
            name: "satz_interview".into(),
            is_error: true,
            body: "denied by the operator".into(),
        }),
        "{seen:?}"
    );
    assert!(
        seen.contains(&Seen::Text("I left it alone.".into())),
        "{seen:?}"
    );
    let answer = running
        .fake
        .stdin()
        .into_iter()
        .find(|l| l["type"] == "control_response")
        .expect("the app answered the request");
    assert_eq!(answer["response"]["response"]["behavior"], "deny");
    assert_eq!(
        answer["response"]["response"]["message"],
        "denied by the operator"
    );
    running.session.close().await;
}

#[tokio::test]
async fn for_the_session_answers_the_second_call_without_a_card() {
    let mut running = start(&["tool-twice"], false).await;
    let (result, seen) = turn(
        &mut running.session,
        "answer both",
        Approval::ForSession,
        false,
    )
    .await;
    result.expect("the turn ends");
    let cards = seen
        .iter()
        .filter(|s| matches!(s, Seen::Pending(_)))
        .count();
    let results = seen
        .iter()
        .filter(|s| matches!(s, Seen::Result { .. }))
        .count();
    assert_eq!(cards, 1, "one card for two calls: {seen:?}");
    assert_eq!(results, 2, "{seen:?}");
    let answers: Vec<_> = running
        .fake
        .stdin()
        .into_iter()
        .filter(|l| l["type"] == "control_response")
        .collect();
    assert_eq!(answers.len(), 2);
    assert!(
        answers
            .iter()
            .all(|a| a["response"]["response"]["behavior"] == "allow")
    );
    running.session.close().await;
}

#[tokio::test]
async fn the_plan_usage_arrives_as_a_notice() {
    let mut running = start(&["rate-limit-turn"], false).await;
    let (result, seen) = turn(&mut running.session, "hello", Approval::Deny, false).await;
    result.expect("the turn ends");
    let Some(Seen::Notice(notice)) = seen.iter().find(|s| matches!(s, Seen::Notice(_))) else {
        panic!("no plan notice: {seen:?}");
    };
    assert!(
        notice.starts_with("Claude plan: 51% of the seven-day limit used"),
        "{notice}"
    );
    assert!(notice.contains("resets in"), "{notice}");
    running.session.close().await;
}

#[tokio::test]
async fn the_turn_cap_ends_the_turn_rather_than_failing_it() {
    let mut running = start(&["max-turns"], false).await;
    let (result, seen) = turn(&mut running.session, "keep going", Approval::Deny, false).await;
    result.expect("the cap is an end, not a failure");
    assert_eq!(seen.last(), Some(&Seen::TurnDone(StopReason::MaxTokens)));
    running.session.close().await;
}

#[tokio::test]
async fn a_turn_the_cli_failed_is_reported_with_what_it_said() {
    let mut running = start(&["failed-turn"], false).await;
    let (result, seen) = turn(&mut running.session, "go", Approval::Deny, false).await;
    let e = result.unwrap_err();
    assert!(matches!(e, ClaudeCodeError::Turn(_)), "{e:?}");
    assert!(e.to_string().contains("the tool server went away"), "{e}");
    assert!(
        matches!(seen.last(), Some(Seen::Failed(text)) if text.contains("the tool server went away")),
        "{seen:?}"
    );
    running.session.close().await;
}

#[tokio::test]
async fn cancel_writes_the_interrupt_and_ends_the_turn_as_cancelled() {
    let mut running = start(&["hanging-turn"], false).await;
    let (result, seen) = turn(&mut running.session, "think about it", Approval::Deny, true).await;
    assert!(
        matches!(result, Err(ClaudeCodeError::Cancelled)),
        "{result:?}"
    );
    assert_eq!(seen.last(), Some(&Seen::Cancelled));
    let interrupts: Vec<_> = running
        .fake
        .stdin()
        .into_iter()
        .filter(|l| l["request"]["subtype"] == "interrupt")
        .collect();
    assert_eq!(interrupts.len(), 1, "one interrupt was written");
    running.session.close().await;
}

#[tokio::test]
async fn a_session_without_the_satz_server_fails_naming_the_status() {
    let mut extra = serde_json::Map::new();
    extra.insert(
        "mcp_status".to_string(),
        serde_json::Value::String("failed".to_string()),
    );
    let mut running = start_with(&["text-turn"], false, extra).await;
    let (result, _) = turn(&mut running.session, "hello", Approval::Deny, false).await;
    let e = result.unwrap_err();
    assert!(
        e.to_string().contains("the satz MCP server is `failed`"),
        "{e}"
    );
    running.session.close().await;
}

#[tokio::test]
async fn the_estate_write_lock_is_held_for_the_whole_turn() {
    // the hanging stream does not end on its own, so the lock can be observed while
    // the turn is genuinely in flight
    let mut running = start(&["hanging-turn"], false).await;
    let estate = Arc::clone(&running.estate);
    let (tx, mut rx) = mpsc::channel(64);
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let turn = tokio::spawn(async move {
        let result = running
            .session
            .run_turn("think".to_string(), tx, cancel)
            .await;
        (running.session, result)
    });
    loop {
        match within(rx.recv()).await {
            Some(AgentEvent::TextDelta(_)) => break,
            Some(_) => continue,
            None => panic!("the turn ended before it streamed"),
        }
    }
    // the guard is never bound — holding it here would be the deadlock this is about
    let free = tokio::time::timeout(std::time::Duration::from_millis(200), estate.write_lock())
        .await
        .is_ok();
    token.cancel();
    while rx.recv().await.is_some() {}
    let (session, result) = within(turn).await.expect("the turn task ends");
    assert!(
        matches!(result, Err(ClaudeCodeError::Cancelled)),
        "{result:?}"
    );
    assert!(!free, "the estate was writable while the turn ran");
    // and it is free again afterwards
    drop(within(estate.write_lock()).await);
    session.close().await;
}

#[tokio::test]
async fn a_fake_that_rejects_the_command_line_fails_the_spawn_with_what_it_said() {
    // the fake refuses a session it was not given `--tools ""` for; the app always
    // passes it, so this drives the same failure by handing the fake nothing to replay
    // and closing its stdin — the session must not report success
    let smoke = copy_smoke();
    let estate = smoke.open().await;
    let tmp = tempfile::tempdir().unwrap();
    let cli = fake(tmp.path(), signed_in(), &[]).locate().await;
    let mut session = within(Session::spawn(
        &cli,
        Arc::clone(&estate),
        options(false).await,
    ))
    .await
    .expect("the session spawns");
    let (tx, mut rx) = mpsc::channel(8);
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let e = within(session.run_turn("hello".to_string(), tx, CancellationToken::new()))
        .await
        .unwrap_err();
    within(drain).await.unwrap();
    assert!(
        e.to_string().contains("closed its output"),
        "a CLI that ends without a result is an error: {e}"
    );
}
