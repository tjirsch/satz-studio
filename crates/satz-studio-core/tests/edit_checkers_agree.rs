//! The two checkers over the same six files — three edits satz accepts, three it
//! refuses — return the same verdict, and for a refusal the same set of
//! `(file, line, kind, message)` tuples, whether they read the findings as JSON over
//! MCP or as the `Debug` of the refusal the CLI exits on. The summaries differ by
//! design: the CLI prints neither an address list nor its findings, so a check that
//! passes carries them in the MCP summary alone.

#[path = "fixtures/edit/support.rs"]
mod support;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use satz_studio_core::cst::TypedValue;
use satz_studio_core::diag::{Diagnostic, Severity};
use satz_studio_core::edit::{CheckFailure, Checker, Edit, EditSession};

type Key = (Option<PathBuf>, Option<u32>, Option<String>, String);

fn key(diags: &[Diagnostic]) -> BTreeSet<Key> {
    diags
        .iter()
        .map(|d| (d.file.clone(), d.line, d.kind.clone(), d.message.clone()))
        .collect()
}

const BUDGET: &str = "presets/organization-budget.satz";

#[tokio::test]
async fn the_two_checkers_agree_on_six_cases() {
    let copy = support::copy_smoke();
    let session = copy.open("showcase.satz").await;
    let (mcp, cli) = support::checkers(&session);
    let es = EditSession::open(&session.main).unwrap();
    let cst = es.cst();
    let cases: Vec<(&str, Edit, bool)> = vec![
        (
            "a number",
            Edit::ReplaceValue {
                node: support::param_value(cst, "audit_retention_days"),
                value: TypedValue::Num("30".to_string()),
            },
            true,
        ),
        (
            "a string",
            Edit::ReplaceParam {
                name: "customer_domain".to_string(),
                value: TypedValue::Str("example.org".to_string()),
            },
            true,
        ),
        (
            "a boolean",
            Edit::ReplaceValue {
                node: support::attr_named(cst, "uniform_bucket_level_access"),
                value: TypedValue::Bool(false),
            },
            true,
        ),
        (
            "a reference nobody declares, in params",
            Edit::ReplaceParam {
                name: "want_optional".to_string(),
                value: TypedValue::Ref("nobody".to_string()),
            },
            false,
        ),
        (
            "a reference nobody declares, in an attribute",
            Edit::ReplaceValue {
                node: support::attr_named(cst, "location"),
                value: TypedValue::Ref("nobody_declares_this".to_string()),
            },
            false,
        ),
        (
            "a reference nobody declares, in a list item",
            Edit::ReplaceValue {
                node: support::value_on_line(
                    cst,
                    support::line_of(cst, "\"roles/iam.securityReviewer\""),
                ),
                value: TypedValue::Ref("not_a_param".to_string()),
            },
            false,
        ),
    ];
    let tmp = copy.file("showcase.studio-tmp.satz");
    for (what, edit, good) in cases {
        let p = es.apply(std::slice::from_ref(&edit)).unwrap();
        std::fs::write(&tmp, p.text()).unwrap();
        let a = support::within(mcp.check(&tmp)).await;
        let b = support::within(cli.check(&tmp)).await;
        match (a, b) {
            (Ok(a), Ok(b)) => {
                assert!(good, "{what}: both checkers passed a bad edit");
                assert!(!a.addresses.is_empty(), "{what}");
                assert!(b.addresses.is_empty(), "{what}");
                assert!(a.written.is_empty() && b.written.is_empty(), "{what}");
                assert_eq!(Path::new(&b.estate), tmp, "{what}");
            }
            (Err(CheckFailure::Refused(a)), Err(CheckFailure::Refused(b))) => {
                assert!(!good, "{what}: both checkers refused a good edit: {a:?}");
                assert_eq!(key(&a), key(&b), "{what}");
                assert_eq!(a.len(), 1, "{what}: {a:?}");
                assert!(a[0].line.is_some(), "{what}: {a:?}");
            }
            (a, b) => panic!("{what}: the checkers disagree — mcp {a:?}, cli {b:?}"),
        }
    }
    std::fs::remove_file(&tmp).unwrap();
}

/// `use_budget = true` spliced into a copy that has no line for the pack, on the temp
/// file the write discipline checks — satz's unadopted-pack finding, whose severity the
/// validation level decides.
fn asks_for_a_pack_it_does_not_use(copy: &support::SmokeCopy, main: &Path) -> PathBuf {
    let proposed = EditSession::open(main)
        .unwrap()
        .apply(&[Edit::ReplaceParam {
            name: "use_budget".to_string(),
            value: TypedValue::Bool(true),
        }])
        .unwrap();
    let tmp = copy.file("smoke.studio-tmp.satz");
    std::fs::write(&tmp, proposed.text()).unwrap();
    tmp
}

#[tokio::test]
async fn a_refusal_is_the_same_findings_through_both_checkers() {
    let copy = support::copy_smoke_at(Some("error"));
    let session = copy.open("smoke.satz").await;
    let tmp = asks_for_a_pack_it_does_not_use(&copy, &session.main);
    let (mcp, cli) = support::checkers(&session);

    let a = support::within(mcp.check(&tmp)).await;
    let b = support::within(cli.check(&tmp)).await;
    let (Err(CheckFailure::Refused(a)), Err(CheckFailure::Refused(b))) = (a, b) else {
        panic!("the checkers did not both refuse a pack the estate asks for and does not use");
    };
    assert_eq!(key(&a), key(&b));
    assert_eq!(a.len(), 1, "{a:?}");
    assert_eq!(a[0].severity, Severity::Error);
    assert_eq!(a[0].kind.as_deref(), Some("unadopted-pack"));
    assert!(
        a[0].message.contains(&format!(
            "`use_budget` is true and this estate has no line for `{BUDGET}` — run `satz merge-presets` to write it"
        )),
        "{}",
        a[0].message
    );
    std::fs::remove_file(&tmp).unwrap();
}

#[tokio::test]
async fn a_check_that_passes_carries_its_warnings_in_the_mcp_summary() {
    let copy = support::copy_smoke();
    let session = copy.open("smoke.satz").await;
    let tmp = asks_for_a_pack_it_does_not_use(&copy, &session.main);
    let (mcp, cli) = support::checkers(&session);

    // the same finding at satz's default level: a warning, so the compile goes on
    let a = support::within(mcp.check(&tmp)).await.unwrap();
    assert!(!a.addresses.is_empty());
    assert_eq!(a.findings.len(), 1, "{:?}", a.findings);
    assert_eq!(a.findings[0].kind, "unadopted-pack");
    assert_eq!(
        a.findings[0].severity,
        satz_studio_core::satz::reports::FindingSeverity::Warning
    );
    assert!(
        a.findings[0].message.contains(BUDGET),
        "{:?}",
        a.findings[0]
    );

    // the CLI prints its findings as sentences and returns none, as with the addresses
    let b = support::within(cli.check(&tmp)).await.unwrap();
    assert!(b.addresses.is_empty());
    assert!(b.findings.is_empty());
    std::fs::remove_file(&tmp).unwrap();
}
