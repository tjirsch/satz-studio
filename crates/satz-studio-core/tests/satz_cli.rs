//! `SatzCli`: `--format json` typed, lines streamed as they arrive, cancellation —
//! against the fixture estate with the installed satz.

use std::path::PathBuf;
use std::time::Duration;

use satz_studio_core::satz::reports::QuestionsReport;
use satz_studio_core::satz::{CliLine, SatzBinary, SatzCli, SatzError};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

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

async fn cli() -> SatzCli {
    let bin = SatzBinary::locate(None).await.unwrap();
    SatzCli::new(bin, fixture())
}

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

#[tokio::test]
async fn questions_as_json_is_typed() {
    let cli = cli().await;
    let report: QuestionsReport = tokio::time::timeout(
        TIME_BOX,
        cli.json(&args(&["questions", "smoke.satz", "--format", "json"])),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(report.summary.total, 27);
    assert_eq!(report.questions.len(), 27);
    assert!(report.estate.ends_with("smoke.satz"), "{}", report.estate);
}

#[tokio::test]
async fn a_failing_json_command_is_an_exit_error_carrying_stderr() {
    let cli = cli().await;
    let err = tokio::time::timeout(
        TIME_BOX,
        cli.json::<QuestionsReport>(&args(&["questions", "nope.satz", "--format", "json"])),
    )
    .await
    .unwrap()
    .unwrap_err();
    match err {
        SatzError::Exit {
            command,
            status,
            stderr,
        } => {
            assert_eq!(command, "questions nope.satz --format json");
            assert!(!status.success());
            assert!(stderr.contains("nope.satz"), "{stderr}");
        }
        other => panic!("expected Exit, got {other:?}"),
    }
}

#[tokio::test]
async fn a_missing_estate_streams_stderr_and_exits_non_zero() {
    let cli = cli().await;
    let (tx, mut rx) = mpsc::channel(16);
    let collect = tokio::spawn(async move {
        let mut lines = Vec::new();
        while let Some(line) = rx.recv().await {
            lines.push(line);
        }
        lines
    });
    let status = tokio::time::timeout(
        TIME_BOX,
        cli.run(
            &args(&["questions", "nope.satz"]),
            tx,
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!status.success(), "{status}");
    let lines = collect.await.unwrap();
    assert!(
        lines
            .iter()
            .any(|l| matches!(l, CliLine::Stderr(text) if text.contains("nope.satz"))),
        "{lines:?}"
    );
}

#[tokio::test]
async fn a_closed_receiver_ends_the_forwarding_not_the_command() {
    let cli = cli().await;
    let (tx, rx) = mpsc::channel(1);
    drop(rx);
    let status = tokio::time::timeout(
        TIME_BOX,
        cli.run(
            &args(&["questions", "smoke.satz", "--format", "json"]),
            tx,
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(status.success(), "{status}");
}

#[tokio::test]
async fn cancellation_returns_cancelled_or_the_command_finished_first() {
    let cli = cli().await;
    let (tx, mut rx) = mpsc::channel(16);
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let cancel = CancellationToken::new();
    let run = {
        let cancel = cancel.clone();
        async move {
            cli.run(&args(&["questions", "smoke.satz"]), tx, cancel)
                .await
        }
    };
    let run = tokio::spawn(run);
    tokio::time::sleep(Duration::from_millis(5)).await;
    cancel.cancel();
    let result = tokio::time::timeout(TIME_BOX, run).await.unwrap().unwrap();
    match result {
        Err(SatzError::Cancelled) | Ok(_) => {}
        Err(other) => panic!("expected Cancelled or a finished command, got {other:?}"),
    }
    drain.await.unwrap();
}
