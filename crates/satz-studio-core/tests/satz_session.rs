//! `EstateSession` over the fixture estate: the identity read from `satz_open`, a
//! tool call typed, and the one-shot script for the terminal.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use satz_studio_core::estate::EstateDir;
use satz_studio_core::satz::reports::QuestionsReport;
use satz_studio_core::satz::{Allow, EstateSession, SatzBinary};

const TIME_BOX: Duration = Duration::from_secs(60);

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("smoke")
        .canonicalize()
        .unwrap()
}

async fn open() -> Arc<EstateSession> {
    let bin = SatzBinary::locate(None).await.unwrap();
    let dir = EstateDir::open(&fixture()).unwrap();
    tokio::time::timeout(
        TIME_BOX,
        EstateSession::open(&bin, dir, PathBuf::from("smoke.satz"), Allow::ReadWrite),
    )
    .await
    .unwrap()
    .unwrap()
}

#[tokio::test]
async fn the_session_reads_the_identity_and_answers_a_tool_call() {
    let session = open().await;
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
    assert_eq!(session.tools().len(), 25);
    assert!(session.tool_info("satz_questions").is_some());
    assert!(!session.instructions().is_empty());
    assert!(!session.guide().is_empty());
    assert!(
        !session.mcp_stderr_backlog().is_empty(),
        "the banner goes to stderr"
    );
    let _follow = session.mcp_stderr();

    let outcome = tokio::time::timeout(
        TIME_BOX,
        session.tool("satz_questions", serde_json::Map::new()),
    )
    .await
    .unwrap()
    .unwrap();
    let report: QuestionsReport = outcome.typed("satz_questions").unwrap();
    assert_eq!(report.summary.total, 27);

    let _guard = session.write_lock().await;
}

#[tokio::test]
async fn external_command_writes_an_executable_script_into_the_estate() {
    let session = open().await;
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
        text.contains(&format!("cd \"{}\"", session.dir.dir.display()))
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
