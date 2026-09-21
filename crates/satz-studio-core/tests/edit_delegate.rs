//! `Snapshot::delegate` over fake tool outcomes, no satz involved: a call satz refuses
//! after it has written the file is restored and the refusal says so; a refusal that
//! wrote nothing is satz's sentence alone; a call that ends without a result is held to
//! the same comparison, a deleted file included; a call that landed and whose check
//! refuses is rolled back.

use std::path::Path;

use satz_studio_core::edit::{
    Cause, CheckFailure, CheckFuture, Checker, CommitError, Delegated, Restore, Rollback, Snapshot,
};
use satz_studio_core::satz::{SatzError, ToolOutcome};

const BEFORE: &str = "params {\n  use_central_alerts = false\n}\n";
const AFTER: &str = "params {\n  use_central_alerts = true\n}\n";
const REFUSAL: &str = "interview: presets/monitoring/organization-cis-log-alerts-central.satz:30: unknown param 'logsink_project_id'";

/// A checker for calls that do not land: reaching it fails the test.
struct Unreached;

impl Checker for Unreached {
    fn check<'a>(&'a self, estate: &'a Path) -> CheckFuture<'a> {
        Box::pin(async move { panic!("a call that did not land was checked: {estate:?}") })
    }
}

/// A checker that refuses every file with no diagnostics.
struct Refuses;

impl Checker for Refuses {
    fn check<'a>(&'a self, _estate: &'a Path) -> CheckFuture<'a> {
        Box::pin(async { Err(CheckFailure::Refused(Vec::new())) })
    }
}

fn estate() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.satz");
    std::fs::write(&path, BEFORE).unwrap();
    (dir, path)
}

fn refused() -> ToolOutcome {
    ToolOutcome {
        structured: None,
        text: REFUSAL.to_string(),
        is_error: true,
    }
}

fn not_landed(d: Delegated) -> satz_studio_core::edit::NotLanded {
    match d {
        Delegated::NotLanded(n) => n,
        other => panic!("the call landed: {other:?}"),
    }
}

#[tokio::test]
async fn a_refusal_that_had_changed_the_file_is_restored_and_says_so() {
    let (_dir, path) = estate();
    let snapshot = Snapshot::take(&path).unwrap();
    let call = async {
        std::fs::write(&path, AFTER).unwrap();
        Ok(refused())
    };
    let n = not_landed(snapshot.delegate(call, &Unreached).await);
    assert!(matches!(n.cause, Cause::Refused(_)), "{n:?}");
    assert!(
        matches!(&n.restore, Restore::Restored(p) if p == &std::path::absolute(&path).unwrap()),
        "{n:?}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BEFORE);
    assert_eq!(
        n.message("satz_interview"),
        format!("{REFUSAL} — satz refused and had changed new.satz; the file is back as it was")
    );
}

#[tokio::test]
async fn a_refusal_that_wrote_nothing_is_satz_s_sentence_alone() {
    let (_dir, path) = estate();
    let snapshot = Snapshot::take(&path).unwrap();
    let n = not_landed(snapshot.delegate(async { Ok(refused()) }, &Unreached).await);
    assert!(matches!(n.restore, Restore::Untouched), "{n:?}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BEFORE);
    assert_eq!(n.message("satz_interview"), REFUSAL);
}

#[tokio::test]
async fn a_call_that_ended_without_a_result_is_compared_and_restored_too() {
    // the session died after satz had written
    let (_dir, path) = estate();
    let snapshot = Snapshot::take(&path).unwrap();
    let call = async {
        std::fs::write(&path, AFTER).unwrap();
        Err(SatzError::Closed("killed".to_string()))
    };
    let n = not_landed(snapshot.delegate(call, &Unreached).await);
    assert!(
        matches!(n.cause, Cause::Failed(SatzError::Closed(_))),
        "{n:?}"
    );
    assert!(matches!(n.restore, Restore::Restored(_)), "{n:?}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BEFORE);
    let message = n.message("satz_add_pack");
    assert!(message.starts_with("satz_add_pack: "), "{message}");
    assert!(
        message.ends_with(
            " — the call ended without a result and had changed new.satz; the file is back as it was"
        ),
        "{message}"
    );

    // … and died before it wrote: the file untouched, the error alone
    let snapshot = Snapshot::take(&path).unwrap();
    let call = async { Err(SatzError::Closed("killed".to_string())) };
    let n = not_landed(snapshot.delegate(call, &Unreached).await);
    assert!(matches!(n.restore, Restore::Untouched), "{n:?}");
    assert_eq!(
        n.message("satz_add_pack"),
        format!("satz_add_pack: {}", SatzError::Closed("killed".to_string()))
    );

    // a file the call removed counts as changed
    let snapshot = Snapshot::take(&path).unwrap();
    let call = async {
        std::fs::remove_file(&path).unwrap();
        Ok(refused())
    };
    let n = not_landed(snapshot.delegate(call, &Unreached).await);
    assert!(matches!(n.restore, Restore::Restored(_)), "{n:?}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BEFORE);
}

#[tokio::test]
async fn a_call_that_landed_and_whose_check_refuses_is_rolled_back() {
    let (_dir, path) = estate();
    let snapshot = Snapshot::take(&path).unwrap();
    let call = async {
        std::fs::write(&path, AFTER).unwrap();
        Ok(ToolOutcome {
            structured: None,
            text: String::new(),
            is_error: false,
        })
    };
    match snapshot.delegate(call, &Refuses).await {
        Delegated::RolledBack {
            error: CommitError::Rollback(Rollback::Check(diags)),
            ..
        } => assert!(diags.is_empty()),
        other => panic!("{other:?}"),
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BEFORE);
}
