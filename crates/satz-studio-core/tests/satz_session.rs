//! `EstateSession` over a copy of the smoke estate: the root `satz mcp-config` renders
//! for it, the identity read from `satz_open`, a tool call typed, a write the window
//! makes at a ceiling the operator's setting does not reach, and the one-shot script for
//! the terminal.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use satz_studio_core::estate::EstateDir;
use satz_studio_core::satz::mcp_config::{self, Client, Run};
use satz_studio_core::satz::reports::{PrerequisitesResult, QuestionsReport};
use satz_studio_core::satz::{Allow, EstateSession, SatzBinary};

#[path = "fixtures/edit/support.rs"]
mod support;

const TIME_BOX: Duration = Duration::from_secs(60);

async fn open(copy: &support::SmokeCopy) -> Arc<EstateSession> {
    let bin = SatzBinary::locate(None).await.unwrap();
    let dir = EstateDir::open(&copy.root).unwrap();
    tokio::time::timeout(
        TIME_BOX,
        EstateSession::open(&bin, dir, PathBuf::from("smoke.satz")),
    )
    .await
    .unwrap()
    .unwrap()
}

#[tokio::test]
async fn the_session_reads_the_identity_and_answers_a_tool_call() {
    let copy = support::copy_smoke();
    let session = open(&copy).await;
    assert!(session.main.is_absolute());
    assert!(
        session.main.ends_with("smoke.satz"),
        "{}",
        session.main.display()
    );
    assert_eq!(session.deployment_mode(), Some("local"));
    assert_eq!(
        session.runs_as(),
        None,
        "the smoke estate impersonates nothing"
    );

    let outcome = tokio::time::timeout(
        TIME_BOX,
        session.tool("satz_questions", serde_json::Map::new()),
    )
    .await
    .unwrap()
    .unwrap();
    let report: QuestionsReport = outcome.typed("satz_questions").unwrap();
    assert_eq!(report.summary.total, 28);

    let _guard = session.write_lock().await;
}

/// The root the window's own `satz mcp` is confined to is the one `satz mcp-config`
/// writes into an agent's configuration for the same estate: the estate's directory,
/// whatever the directories its config names reach into.
#[tokio::test]
async fn the_session_is_rooted_where_mcp_config_roots_an_agent() {
    let copy = support::copy_smoke();
    let session = open(&copy).await;
    assert_eq!(session.root, copy.root);
    let printed = tokio::time::timeout(
        TIME_BOX,
        mcp_config::run(
            &session.cli,
            "smoke.satz",
            Client::ClaudeDesktop,
            Allow::Read,
            Run::Show,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(mcp_config::root(&printed).unwrap(), session.root);
}

/// The window's own writes need `write` whatever ceiling Settings holds for the agent:
/// the session is started at `Allow::STUDIO`, and a writing tool runs on it.
#[tokio::test]
async fn the_window_writes_whatever_ceiling_the_agent_is_given() {
    assert_eq!(Allow::STUDIO, Allow::ReadWrite);
    let copy = support::copy_smoke();
    let session = open(&copy).await;
    let mut args = serde_json::Map::new();
    args.insert("report_only".to_string(), serde_json::Value::Bool(false));
    let outcome = tokio::time::timeout(TIME_BOX, session.tool("satz_update_prerequisites", args))
        .await
        .unwrap()
        .unwrap();
    assert!(!outcome.is_error, "{}", outcome.text);
    outcome
        .typed::<PrerequisitesResult>("satz_update_prerequisites")
        .unwrap();
}

#[tokio::test]
async fn external_command_writes_an_executable_script_into_the_estate() {
    let copy = support::copy_smoke();
    let session = open(&copy).await;
    let script = session
        .external_command(&[
            "apply".to_string(),
            "--target".to_string(),
            "a b".to_string(),
        ])
        .unwrap();
    let text = std::fs::read_to_string(&script).unwrap();
    // read before the file goes; only the unix assertion below reads it
    #[cfg(unix)]
    let mode = std::fs::metadata(&script).unwrap().permissions();
    std::fs::remove_file(&script).unwrap();
    assert!(
        script.parent().unwrap().ends_with("run"),
        "{}",
        script.display()
    );
    assert!(text.contains("--config . apply --target \"a b\""), "{text}");
    assert!(
        text.contains(&format!("cd '{}'", session.dir.dir.display()))
            || text.contains(&format!("cd /d \"{}\"", session.dir.dir.display())),
        "{text}"
    );
    assert!(
        text.contains(&session.cli.bin.path.display().to_string()),
        "{text}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert!(text.starts_with("#!/bin/sh\n"));
        assert_eq!(mode.mode() & 0o777, 0o755, "script mode {:o}", mode.mode());
    }
}
