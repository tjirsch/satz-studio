//! The two checkers over the same six files — three edits satz accepts, three it
//! refuses — return the same verdict, and for a refusal the same set of
//! `(line, message)` pairs. The summaries differ by design: the CLI prints no address
//! list.

#[path = "fixtures/edit/support.rs"]
mod support;

use std::collections::BTreeSet;
use std::path::Path;

use satz_studio_core::cst::TypedValue;
use satz_studio_core::diag::Diagnostic;
use satz_studio_core::edit::{CheckFailure, Checker, Edit, EditSession};

fn key(diags: &[Diagnostic]) -> BTreeSet<(Option<u32>, String)> {
    diags.iter().map(|d| (d.line, d.message.clone())).collect()
}

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
                node: support::attr_on_line(cst, 144, "uniform_bucket_level_access"),
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
                node: support::attr_on_line(cst, 143, "location"),
                value: TypedValue::Ref("nobody_declares_this".to_string()),
            },
            false,
        ),
        (
            "a reference nobody declares, in a list item",
            Edit::ReplaceValue {
                node: support::value_on_line(cst, 116),
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
