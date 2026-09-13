//! `ReplaceParam` writes the bytes satz's own writer writes. On a line that is not
//! column-aligned the two are identical, in place and appended; on an aligned line
//! they differ only in the whitespace before `=`, which satz's `bind` drops and
//! `ReplaceParam` keeps. satz's version comes from `satz_interview {answers}` through
//! the session, on the same copy, after the edit session has read the bytes.

#[path = "fixtures/edit/support.rs"]
mod support;

use satz_studio_core::cst::TypedValue;
use satz_studio_core::edit::{Edit, EditSession};

#[tokio::test]
async fn on_unaligned_params_replace_param_and_satz_write_the_same_bytes() {
    let copy = support::copy_smoke();
    let session = copy.open("smoke.satz").await;
    // The premise is the line's shape, not the fixture's: whatever alignment the
    // smoke estate carries, this copy binds `logsink_project_id` with one space
    // on each side of `=`, which is the shape satz's `bind` writes back.
    let unaligned: String = support::read(&session.main)
        .lines()
        .map(|l| {
            if l.trim_start().starts_with("logsink_project_id") {
                "  logsink_project_id = \"corp-log-infra-002\""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    std::fs::write(&session.main, unaligned).unwrap();
    let ours = EditSession::open(&session.main)
        .unwrap()
        .apply(&[
            Edit::ReplaceParam {
                name: "logsink_project_id".to_string(),
                value: TypedValue::Str("corp-log-infra-002".to_string()),
            },
            Edit::ReplaceParam {
                name: "logsink_retention_days".to_string(),
                value: TypedValue::Num("400".to_string()),
            },
        ])
        .unwrap();
    let report = support::interview(
        &session,
        serde_json::json!({"logsink_project_id": "corp-log-infra-002", "logsink_retention_days": 400}),
    )
    .await;
    assert_eq!(report.written, 2);
    let satz = support::read(&session.main);
    assert!(
        satz.contains("\n  logsink_project_id = \"corp-log-infra-002\"\n"),
        "{satz}"
    );
    assert!(
        satz.contains("\n  logsink_retention_days = 400\n}\n"),
        "{satz}"
    );
    assert_eq!(ours.text(), satz);
}

#[tokio::test]
async fn on_an_aligned_line_the_two_differ_only_in_the_whitespace_before_the_equals_sign() {
    let copy = support::copy_smoke();
    let session = copy.open("showcase.satz").await;
    let es = EditSession::open(&session.main).unwrap();
    let ours = es
        .apply(&[Edit::ReplaceParam {
            name: "default_region".to_string(),
            value: TypedValue::Str("europe-west1".to_string()),
        }])
        .unwrap();
    let report = support::interview(
        &session,
        serde_json::json!({"default_region": "europe-west1"}),
    )
    .await;
    assert_eq!(report.written, 1);
    let satz = support::read(&session.main);

    let before: Vec<&str> = es.text().lines().collect();
    let a: Vec<&str> = ours.text().lines().collect();
    let b: Vec<&str> = satz.lines().collect();
    assert_eq!(a.len(), before.len());
    assert_eq!(b.len(), before.len());
    const LINE: usize = 17; // `  default_region           = "europe-west3"`
    for i in 0..before.len() {
        if i == LINE {
            continue;
        }
        assert_eq!(a[i], before[i], "line {}", i + 1);
        assert_eq!(b[i], before[i], "line {}", i + 1);
    }
    assert_eq!(a[LINE], "  default_region           = \"europe-west1\"");
    assert_eq!(b[LINE], "  default_region = \"europe-west1\"");
    let (a_head, a_tail) = a[LINE].split_once('=').unwrap();
    let (b_head, b_tail) = b[LINE].split_once('=').unwrap();
    assert_eq!(a_tail, b_tail);
    assert_eq!(a_head.trim_end(), b_head.trim_end());
    assert_ne!(a_head, b_head);
    // `ReplaceParam` keeps the alignment: the head is the original's
    assert_eq!(a_head, before[LINE].split_once('=').unwrap().0);
}
