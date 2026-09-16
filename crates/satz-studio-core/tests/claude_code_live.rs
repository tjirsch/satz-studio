//! One real Claude Code turn against the smoke estate, opt-in:
//! `SATZ_STUDIO_LIVE_CLAUDE_CODE=1` with a `claude` on the machine that is signed in
//! to a claude.ai account. Without it the test prints a note and passes. It spends the
//! subscription's quota, so it is never part of a normal run.

use std::sync::Arc;

use satz_studio_core::llm::AgentEvent;
use satz_studio_core::llm::claude_code::{ClaudeCodeCli, Session, SessionOptions};
use satz_studio_core::satz::Allow;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[path = "fixtures/claude_code/support.rs"]
mod support;

use support::{copy_smoke, satz_binary, within};

#[tokio::test]
async fn a_real_turn_reads_the_estate_through_the_satz_tools() {
    if std::env::var("SATZ_STUDIO_LIVE_CLAUDE_CODE").as_deref() != Ok("1") {
        println!(
            "skipped: set SATZ_STUDIO_LIVE_CLAUDE_CODE=1, with a signed-in `claude`, to run one real turn"
        );
        return;
    }
    let cli = ClaudeCodeCli::locate(None)
        .await
        .expect("the Claude Code CLI is installed");
    let status = cli
        .auth_status()
        .await
        .expect("`claude auth status` answers");
    assert!(
        status.logged_in,
        "sign in first: run `claude auth login` in a terminal"
    );

    let smoke = copy_smoke();
    let estate = smoke.open().await;
    let options = SessionOptions {
        model: None,
        max_turns: Some(6),
        resume: None,
        auto_approve_writes: false,
        satz_binary: satz_binary().await,
        allow: Allow::Read,
        log: None,
    };
    let mut session = within(Session::spawn(&cli, Arc::clone(&estate), options))
        .await
        .expect("the session spawns");

    let (tx, mut rx) = mpsc::channel(256);
    let collector = tokio::spawn(async move {
        let mut tools = Vec::new();
        let mut text = String::new();
        while let Some(event) = rx.recv().await {
            match event {
                AgentEvent::TextDelta(piece) => text.push_str(&piece),
                AgentEvent::ToolResult { name, outcome, .. } => {
                    tools.push((name, outcome.is_error));
                }
                // every tool of a read ceiling is read-only, so none needs a card
                AgentEvent::ToolCallPending { name, .. } => {
                    panic!("a read-only turn asked for approval: {name}")
                }
                _ => {}
            }
        }
        (tools, text)
    });
    let result = within(session.run_turn(
        "Open this estate and tell me which questions are open. Do not write anything.".to_string(),
        tx,
        CancellationToken::new(),
    ))
    .await;
    let (tools, text) = collector.await.expect("the collector ends");
    session.close().await;

    result.expect("the turn ends");
    println!("claude code said: {text}");
    println!("tools: {tools:?}");
    assert!(
        tools.iter().any(|(name, _)| name == "satz_questions"),
        "no satz_questions result in {tools:?}"
    );
}
