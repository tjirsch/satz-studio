//! `SatzCli`: the report a reporting command writes, read back and typed; lines
//! streamed as they arrive; cancellation — against the fixture estate with the
//! installed satz.

use std::path::{Path, PathBuf};
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

/// What the estate directory holds, sorted: a reporting call writes into none of it.
fn listing(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    out.sort();
    out
}

/// The `--out` a call named, from the command its error carries. It is the last
/// argument, so everything after the flag is the path.
fn destination(command: &str) -> PathBuf {
    PathBuf::from(
        command
            .split_once(" --out ")
            .expect("the call named an --out")
            .1,
    )
}

#[tokio::test]
async fn a_report_is_read_from_the_file_the_command_wrote() {
    let cli = cli().await;
    let report: QuestionsReport = tokio::time::timeout(
        TIME_BOX,
        cli.json_report(&args(&["questions", "smoke.satz"])),
    )
    .await
    .unwrap()
    .unwrap();
    // estate-core's questions include `compliance_frameworks` (satz v0.73.0)
    assert_eq!(report.summary.total, 28);
    assert_eq!(report.questions.len(), 28);
    assert!(report.estate.ends_with("smoke.satz"), "{}", report.estate);
}

/// The contract satz's ADR 0021 puts on a reporting call — one format, one file — as
/// the app makes it: the report comes back parsed, the estate the command ran on is
/// untouched, and the file the app named is gone with the directory it stood in. The
/// failing call is what names that directory: its error carries the command it ran.
#[tokio::test]
async fn a_reporting_call_leaves_nothing_behind() {
    let cli = cli().await;
    let estate = fixture();
    let before = listing(&estate);
    let report: QuestionsReport = tokio::time::timeout(
        TIME_BOX,
        cli.json_report(&args(&["questions", "smoke.satz"])),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(report.summary.total > 0);
    assert_eq!(listing(&estate), before, "the estate is untouched");

    let err = tokio::time::timeout(
        TIME_BOX,
        cli.json_report::<QuestionsReport>(&args(&["questions", "nope.satz"])),
    )
    .await
    .unwrap()
    .unwrap_err();
    let SatzError::Exit { command, .. } = &err else {
        panic!("expected Exit, got {err:?}")
    };
    let out = destination(command);
    assert!(!out.exists(), "{} is still there", out.display());
    let dir = out.parent().unwrap();
    assert!(!dir.exists(), "{} is still there", dir.display());
    assert_eq!(listing(&estate), before, "the estate is untouched");
}

#[tokio::test]
async fn a_failing_report_is_an_exit_error_carrying_stderr() {
    let cli = cli().await;
    let err = tokio::time::timeout(
        TIME_BOX,
        cli.json_report::<QuestionsReport>(&args(&["questions", "nope.satz"])),
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
            assert!(
                command.starts_with("questions nope.satz --format json --out "),
                "{command}"
            );
            assert!(!status.success());
            assert!(stderr.contains("nope.satz"), "{stderr}");
        }
        other => panic!("expected Exit, got {other:?}"),
    }
}

#[tokio::test]
async fn a_missing_estate_streams_stderr_and_exits_non_zero() {
    let cli = cli().await;
    let out = tempfile::tempdir().unwrap();
    let report = out.path().join("questions.json");
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
            &args(&[
                "questions",
                "nope.satz",
                "--format",
                "json",
                "--out",
                &report.display().to_string(),
            ]),
            tx,
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!status.success(), "{status}");
    assert!(!report.exists(), "a refused command writes no report");
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
    let out = tempfile::tempdir().unwrap();
    let report = out.path().join("questions.json");
    let (tx, rx) = mpsc::channel(1);
    drop(rx);
    let status = tokio::time::timeout(
        TIME_BOX,
        cli.run(
            &args(&[
                "questions",
                "smoke.satz",
                "--format",
                "json",
                "--out",
                &report.display().to_string(),
            ]),
            tx,
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(status.success(), "{status}");
    assert!(report.exists(), "the command wrote its one file");
}

#[tokio::test]
async fn cancellation_returns_cancelled_or_the_command_finished_first() {
    let cli = cli().await;
    let out = tempfile::tempdir().unwrap();
    let report = out.path().join("questions.json");
    let (tx, mut rx) = mpsc::channel(16);
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let cancel = CancellationToken::new();
    let run = {
        let cancel = cancel.clone();
        let argv = args(&[
            "questions",
            "smoke.satz",
            "--format",
            "json",
            "--out",
            &report.display().to_string(),
        ]);
        async move { cli.run(&argv, tx, cancel).await }
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

/// Every line a run printed, both streams, and the file it wrote when it wrote one:
/// what the command log shows for that run.
async fn log_of(cli: &SatzCli, argv: &[String], wrote: Option<&Path>) -> Vec<String> {
    let (tx, mut rx) = mpsc::channel(256);
    let collect = tokio::spawn(async move {
        let mut lines = Vec::new();
        while let Some(line) = rx.recv().await {
            lines.push(match line {
                CliLine::Stdout(s) | CliLine::Stderr(s) => s,
            });
        }
        lines
    });
    let status = tokio::time::timeout(TIME_BOX, cli.run(argv, tx, CancellationToken::new()))
        .await
        .unwrap()
        .unwrap();
    assert!(status.success(), "{argv:?}: {status}");
    let mut lines = collect.await.unwrap();
    if let Some(path) = wrote {
        lines.extend(
            std::fs::read_to_string(path)
                .unwrap()
                .lines()
                .map(str::to_string),
        );
    }
    lines
}

/// The one-click commands beside the palette — `whoami`, `transpile --check` and
/// `questions --format text` on the estate — put satz's own prose into the log: lines
/// of text, and none that opens a JSON object or array. `whoami` runs `--offline`
/// here, which needs no credential; the button runs it online.
#[tokio::test]
async fn the_one_click_commands_print_prose() {
    let cli = cli().await;
    let out = tempfile::tempdir().unwrap();
    let questions = out.path().join("questions.txt");
    let runs: [(Vec<String>, Option<&Path>, &str); 3] = [
        (
            args(&["whoami", "smoke.satz", "--offline"]),
            None,
            "runs as:",
        ),
        (
            args(&["transpile", "smoke.satz", "--check"]),
            None,
            "transpile --check: OK",
        ),
        (
            args(&[
                "questions",
                "smoke.satz",
                "--format",
                "text",
                "--out",
                &questions.display().to_string(),
            ]),
            Some(&questions),
            "essential_contacts_email",
        ),
    ];
    for (argv, wrote, says) in runs {
        let lines = log_of(&cli, &argv, wrote).await;
        assert!(
            lines.iter().any(|l| l.contains(says)),
            "{argv:?}: {lines:#?}"
        );
        for l in &lines {
            let t = l.trim_start();
            assert!(
                !t.starts_with('{') && !t.starts_with('['),
                "{argv:?} printed JSON: {l}"
            );
        }
    }
}
