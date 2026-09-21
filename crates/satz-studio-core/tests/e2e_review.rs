//! `satz review-pack` end to end with the installed satz, the way the Packs view runs it:
//! the CLI with the estate's directory as `--config`, over an estate the smoke matrix's
//! answers complete. A pack of satz's own library clears the bar; the broken fixture
//! (`tests/fixtures/review/team-access.satz`) does not, and each of its findings is a
//! diagnostic at a line of the pack's own text. The private destination then places the
//! reviewed bytes as `<stem>.local.satz` with the estate checked over the session, and
//! refuses a second placement over other text. The library it places into is a scratch
//! directory: the estate's own `presets_dir` is `vendor/satz/presets`, which no test
//! writes.

#[path = "fixtures/e2e/support.rs"]
mod support;

use std::path::PathBuf;
use std::sync::Arc;

use satz_studio_core::diag::Severity;
use satz_studio_core::edit::McpChecker;
use satz_studio_core::satz::review::{self, PlaceError, Placed};

fn broken_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("review")
        .join("team-access.satz")
}

#[tokio::test]
async fn a_library_pack_clears_the_bar_and_a_broken_one_is_findings_at_its_lines() {
    let estate = support::estate_dir(None);
    let cli = estate.cli().await;

    let budget = support::vendor()
        .join("presets")
        .join("organization-budget.satz");
    let clean = support::within(review::review(&cli, &budget, None))
        .await
        .unwrap();
    assert!(clean.review.passed(), "{:#?}", clean.review.findings);
    assert_eq!(clean.review.folded_into, "synthetic");
    assert!(!clean.review.emits.is_empty());

    // a copy, so the file the review names is a scratch one
    let scratch = support::scratch();
    let pack = scratch.path().join("team-access.satz");
    std::fs::copy(broken_fixture(), &pack).unwrap();
    let broken = support::within(review::review(&cli, &pack, None))
        .await
        .unwrap();
    assert!(!broken.review.passed());
    assert_eq!(broken.text, support::read(&pack));

    let diags = broken.diagnostics();
    assert_eq!(diags.len(), broken.review.findings.len());
    let membership = diags
        .iter()
        .find(|d| d.message.starts_with("declares the membership"))
        .unwrap_or_else(|| panic!("no membership finding: {diags:#?}"));
    assert_eq!(membership.severity, Severity::Error);
    // the line satz names is the membership's own block in the pack's text
    let line = membership.line.expect("anchored to a line") as usize;
    let at = broken.text.lines().nth(line - 1).unwrap_or_default();
    assert!(
        at.trim_start().starts_with("first {"),
        "line {line} is not the membership's block: {at:?}"
    );
    for d in &diags {
        assert_eq!(
            d.file.as_deref().and_then(|f| f.file_name()),
            pack.file_name(),
            "{d:?}"
        );
    }
}

#[tokio::test]
async fn a_reviewed_pack_is_placed_once_and_never_over_other_text() {
    let estate = support::estate_dir(None);
    let main = estate.create_skeleton("C0example.satz").await;
    let session = estate
        .open(main.file_name().unwrap().to_str().unwrap())
        .await;
    support::answer_like_the_smoke_matrix(&session).await;

    let scratch = support::scratch();
    let pack = scratch.path().join("team-access.satz");
    std::fs::copy(broken_fixture(), &pack).unwrap();
    let reviewed = support::within(review::review(&session.cli, &pack, None))
        .await
        .unwrap();

    let library = scratch.path().join("presets");
    std::fs::create_dir_all(&library).unwrap();
    let checker = McpChecker {
        session: Arc::clone(&session),
    };
    let placed = support::within(review::place_private(
        &reviewed,
        &library,
        &session.main,
        &checker,
    ))
    .await
    .unwrap();
    let target = library.join("team-access.local.satz");
    assert!(
        matches!(&placed, Placed::Written { path, .. } if *path == target),
        "{placed:?}"
    );
    assert_eq!(support::read(&target), reviewed.text);

    // placed again: the bytes are there already
    let again = support::within(review::place_private(
        &reviewed,
        &library,
        &session.main,
        &checker,
    ))
    .await
    .unwrap();
    assert_eq!(again, Placed::AlreadyThere(target.clone()));

    // the estate's own fork under that name is never written over
    std::fs::write(&target, "// the estate's own fork\n").unwrap();
    let refused = support::within(review::place_private(
        &reviewed,
        &library,
        &session.main,
        &checker,
    ))
    .await
    .unwrap_err();
    assert!(matches!(refused, PlaceError::Exists(_)), "{refused}");
    assert_eq!(support::read(&target), "// the estate's own fork\n");
}
