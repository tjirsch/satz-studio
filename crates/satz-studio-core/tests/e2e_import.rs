//! `satz import` as the Import door runs it, against the installed satz.
//!
//! The Terraform HCL shape is the one that can be driven end to end offline: a `.tf`
//! file on disk, the real binary, and what it wrote and printed read back the way the
//! app reads it. The LIVE shape is never run here and must never be — it reads a Google
//! organisation with the maintainer's credentials and would put an organisation id, a
//! project id and a billing account into a test's output, every one of which the privacy
//! gate refuses. The legacy YAML shape is driven over satz's own corpus fixtures, which
//! is where the "the conversion is written BESIDE its source" rule is proved.
//!
//! What is under test is the app's half: the argv (in the module's own tests), where the
//! file lands, that it is found by reading the directory rather than by predicting a
//! name, and that satz's report survives the split into sections.

#[path = "fixtures/e2e/support.rs"]
mod support;

use std::path::Path;

use satz_studio_core::satz::import::{ImportShape, YamlKind, satz_files, written_since};
use satz_studio_core::satz::{CliLine, ImportOptions, ImportReport, SatzCli, SatzError};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// The four lines the door is proved against: one `variable`, which the import promotes
/// to a param, and one resource the provider schema knows, which it translates.
const MAIN_TF: &str = r#"variable "region" {
  default = "europe-west3"
}

resource "google_storage_bucket" "b" {
  name     = "acme-bucket"
  location = var.region
}
"#;

/// `satz <argv>` in `dir` with no `--config`, as [`SatzCli::run_in`] runs it behind the
/// door: whether it exited zero, and every line it streamed.
async fn run_in(dir: &Path, args: &[String]) -> (bool, Vec<CliLine>) {
    let bin = support::satz().await;
    let (tx, mut rx) = mpsc::channel(256);
    let collect = tokio::spawn(async move {
        let mut lines = Vec::new();
        while let Some(line) = rx.recv().await {
            lines.push(line);
        }
        lines
    });
    let status = support::within(SatzCli::run_in(
        &bin.path,
        dir,
        args,
        tx,
        CancellationToken::new(),
    ))
    .await
    .unwrap();
    (status.success(), collect.await.unwrap())
}

fn stderr(lines: &[CliLine]) -> String {
    lines
        .iter()
        .filter_map(|l| match l {
            CliLine::Stderr(t) => Some(t.as_str()),
            CliLine::Stdout(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The whole door over the hcl shape: satz writes a file whose name the app never
/// guessed, the read-back finds exactly that file, it declares an estate, and the report
/// keeps what was promoted — with its `file:line` — instead of burying it.
#[tokio::test]
async fn the_hcl_shape_writes_an_estate_the_read_back_finds() {
    let estate = support::estate_dir(None);
    let source = estate.root.join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("main.tf"), MAIN_TF).unwrap();

    let options = ImportOptions {
        shape: ImportShape::Hcl,
        source: "src".to_string(),
        ..Default::default()
    };
    let dir = satz_studio_core::estate::EstateDir::open(&estate.root).unwrap();
    let dirs = options.write_dirs(&dir);
    assert_eq!(dirs, [estate.yaml.clone()].as_slice());
    let before = satz_files(&dirs).unwrap();
    assert!(before.is_empty(), "the estate starts with no .satz file");

    let (ok, lines) = run_in(&estate.root, &options.argv()).await;
    assert!(ok, "{}", stderr(&lines));

    // the name is satz's (`imported-hcl.satz`, which no flag of this run stated) and is
    // found by reading the directory
    let written = written_since(&before, &dirs).unwrap();
    assert_eq!(written.len(), 1, "{written:?}");
    assert_eq!(
        written[0].path.file_name().unwrap().to_string_lossy(),
        "imported-hcl.satz"
    );
    assert!(written[0].declares_estate);
    let text = std::fs::read_to_string(&written[0].path).unwrap();
    assert!(text.contains("estate "), "{text}");
    assert!(text.contains("region"), "{text}");

    let report = ImportReport::of(&lines);
    assert_eq!(report.wrote.len(), 1, "{report:?}");
    assert!(report.wrote[0].contains("imported-hcl.satz"));
    // satz's own "what to do next" sentence, which the result screen shows
    assert!(report.wrote[0].contains("tofu plan"), "{:?}", report.wrote);
    assert!(
        report
            .rest
            .iter()
            .any(|l| l.contains("promoted") && l.contains("src/main.tf:")),
        "the promoted block, with its file and line: {:?}",
        report.rest
    );
    assert!(report.skipped.is_empty() && report.warnings.is_empty());
}

/// A second run over the same source writes the same file again. The read-back is a hash
/// and not a listing, so it answers this as a file WRITTEN rather than as nothing having
/// happened — which is what lets the door open the estate of a re-run.
#[tokio::test]
async fn a_second_import_over_the_same_file_is_still_a_file_written() {
    let estate = support::estate_dir(None);
    let source = estate.root.join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("main.tf"), MAIN_TF).unwrap();
    let options = ImportOptions {
        shape: ImportShape::Hcl,
        source: "src".to_string(),
        ..Default::default()
    };
    let dir = satz_studio_core::estate::EstateDir::open(&estate.root).unwrap();
    let dirs = options.write_dirs(&dir);

    let (ok, lines) = run_in(&estate.root, &options.argv()).await;
    assert!(ok, "{}", stderr(&lines));
    let written = std::fs::read_to_string(dirs[0].join("imported-hcl.satz")).unwrap();

    // the file is changed by hand, so a re-run has something to overwrite
    std::fs::write(
        dirs[0].join("imported-hcl.satz"),
        format!("// edited\n{written}"),
    )
    .unwrap();
    let before = satz_files(&dirs).unwrap();
    assert_eq!(before.len(), 1);

    let (ok, lines) = run_in(&estate.root, &options.argv()).await;
    assert!(ok, "{}", stderr(&lines));
    let again = written_since(&before, &dirs).unwrap();
    assert_eq!(again.len(), 1, "{again:?}");
    assert!(again[0].declares_estate);
}

/// The refusal the form states before a run, held to the refusal satz gives: a raw
/// `.tfstate` is not the document `tofu show -json` writes, whatever it is named, and
/// both halves say so in the same words.
#[tokio::test]
async fn a_raw_tfstate_is_refused_by_the_form_and_by_satz_alike() {
    let estate = support::estate_dir(None);
    let raw = r#"{"version":4,"terraform_version":"1.9.0","resources":[{"mode":"managed","type":"google_storage_bucket","name":"b","instances":[]}]}"#;
    std::fs::write(estate.root.join("state.json"), raw).unwrap();
    let options = ImportOptions {
        shape: ImportShape::State,
        source: "state.json".to_string(),
        ..Default::default()
    };

    let refused = options.check_source(&estate.root).unwrap_err();
    assert!(
        matches!(refused, SatzError::NotAShowDocument(_)),
        "{refused:?}"
    );

    let (ok, lines) = run_in(&estate.root, &options.argv()).await;
    assert!(!ok, "satz took a raw .tfstate");
    let said = stderr(&lines);
    assert!(said.contains("values.root_module"), "{said}");
    assert!(said.contains("tofu show -json"), "{said}");
    assert!(
        refused.to_string().contains("values.root_module")
            && refused.to_string().contains("tofu show -json > state.json"),
        "the form says what satz says: {refused}"
    );
}

/// The legacy YAML shape, over satz's own corpus fixtures: the conversion is written
/// BESIDE its source rather than into `yaml_dir`, a pack conversion declares no estate
/// and an estate conversion does — which is what decides whether anything opens.
#[tokio::test]
async fn the_yaml_shape_writes_beside_its_source_and_only_an_estate_opens() {
    let estate = support::estate_dir(None);
    let corpus = support::vendor()
        .join("tests")
        .join("corpus")
        .join("yaml-estate");
    let legacy = estate.root.join("legacy");
    std::fs::create_dir_all(&legacy).unwrap();
    for name in ["pack.yaml", "main.yaml"] {
        std::fs::copy(corpus.join(name), legacy.join(name)).unwrap();
    }

    let converted = |file: &str, kind: YamlKind| ImportOptions {
        shape: ImportShape::Yaml,
        source: format!("legacy/{file}"),
        kind,
        ..Default::default()
    };
    let dir = satz_studio_core::estate::EstateDir::open(&estate.root).unwrap();
    let dirs = converted("pack.yaml", YamlKind::Pack).write_dirs(&dir);
    assert_eq!(
        dirs,
        [legacy.clone()].as_slice(),
        "beside the source, not in yaml_dir"
    );

    // the pack first: satz refuses an estate that still `use`s a YAML pack
    let before = satz_files(&dirs).unwrap();
    let options = converted("pack.yaml", YamlKind::Pack);
    let (ok, lines) = run_in(&estate.root, &options.argv()).await;
    assert!(ok, "{}", stderr(&lines));
    let written = written_since(&before, &dirs).unwrap();
    assert_eq!(written.len(), 1, "{written:?}");
    assert!(
        !written[0].declares_estate,
        "a converted pack is a file, not an estate"
    );

    let before = satz_files(&dirs).unwrap();
    let options = converted("main.yaml", YamlKind::Estate);
    let (ok, lines) = run_in(&estate.root, &options.argv()).await;
    assert!(ok, "{}", stderr(&lines));
    let written = written_since(&before, &dirs).unwrap();
    assert_eq!(written.len(), 1, "{written:?}");
    assert!(written[0].declares_estate);
    assert_eq!(
        written[0].path.file_name().unwrap().to_string_lossy(),
        "main.satz"
    );
    let report = ImportReport::of(&lines);
    assert_eq!(report.wrote.len(), 1, "{report:?}");
    assert!(
        report.wrote[0].starts_with("converted "),
        "{:?}",
        report.wrote
    );
}
