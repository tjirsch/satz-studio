//! The export of the decisions sheet and the workbook, end to end with the installed
//! satz against the smoke estate: the formats the app offers are exactly the ones satz
//! accepts, and every one of them writes a non-empty file at the named path — the
//! markdown sheet as text, the workbook as a zip.

use std::path::PathBuf;
use std::time::Duration;

use satz_studio_core::satz::{CliLine, SatzBinary, SatzCli, export};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const TIME_BOX: Duration = Duration::from_secs(120);

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

/// The set satz itself names when it refuses a format: clap's `[possible values: …]`
/// line, a source independent of the long help the app reads.
async fn refused_with(cli: &SatzCli) -> Vec<String> {
    let output = tokio::process::Command::new(&cli.bin.path)
        .args(["questions", "smoke.satz", "--format", "no-such-format"])
        .args(["--out", "unused"])
        .current_dir(&cli.config_dir)
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let list = stderr
        .split_once("[possible values: ")
        .and_then(|(_, rest)| rest.split_once(']'))
        .unwrap_or_else(|| panic!("satz named no possible values: {stderr}"))
        .0;
    list.split(',').map(|v| v.trim().to_string()).collect()
}

#[tokio::test]
async fn the_picker_offers_exactly_the_formats_satz_accepts() {
    let cli = cli().await;
    let offered: Vec<String> = tokio::time::timeout(TIME_BOX, export::formats(&cli))
        .await
        .expect("timed out")
        .unwrap()
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(offered, refused_with(&cli).await);
    for needed in ["markdown", "xlsx"] {
        assert!(offered.iter().any(|f| f == needed), "{offered:?}");
    }
}

#[tokio::test]
async fn every_format_writes_its_document_at_the_named_path() {
    let cli = cli().await;
    let out = tempfile::tempdir().unwrap();
    let formats = tokio::time::timeout(TIME_BOX, export::formats(&cli))
        .await
        .expect("timed out")
        .unwrap();
    for format in formats {
        let name = format.name.as_str();
        // the path the dialog would propose, without its extension: the export adds it
        let chosen = out.path().join(format!("smoke-decisions-{name}"));
        let destination = export::destination(&chosen, name);
        let argv = export::args("smoke.satz", name, &destination);
        let (tx, mut rx) = mpsc::channel::<CliLine>(1024);
        let status = tokio::time::timeout(TIME_BOX, cli.run(&argv, tx, CancellationToken::new()))
            .await
            .expect("timed out")
            .unwrap();
        let mut lines = Vec::new();
        while let Ok(line) = rx.try_recv() {
            lines.push(line);
        }
        assert!(status.success(), "{argv:?}: {lines:#?}");
        let size = export::written(&destination).unwrap();
        assert!(size > 0, "{name}");
        let bytes = std::fs::read(&destination).unwrap();
        match name {
            "xlsx" => assert!(bytes.starts_with(b"PK"), "the workbook is not a zip"),
            "pdf" => assert!(bytes.starts_with(b"%PDF"), "the sheet is not a PDF"),
            "markdown" => {
                let text = String::from_utf8(bytes).unwrap();
                assert!(
                    text.starts_with("# Decisions"),
                    "the sheet opens with no Decisions heading"
                );
            }
            _ => {}
        }
    }
    assert!(out.path().join("smoke-decisions-markdown.md").is_file());
    assert!(out.path().join("smoke-decisions-xlsx.xlsx").is_file());
}
