//! `ReplaceParam` writes the bytes satz's own writer writes. satz's `bind` replaces a
//! value in place and leaves its line as the author wrote it; an answer it APPENDS lays
//! the whole params block out as `satz fmt` does, so the block keeps one `=` column.
//! Each case runs both on the same copy — satz's through `satz_interview {answers}`
//! over the session, after the edit session has read the bytes.

#[path = "fixtures/edit/support.rs"]
mod support;

use satz_studio_core::cst::TypedValue;
use satz_studio_core::edit::{Edit, EditSession};

/// The column of the `=` on the line binding `name`.
fn column(text: &str, name: &str) -> Option<usize> {
    text.lines()
        .find(|l| l.trim_start().starts_with(&format!("{name} ")))
        .and_then(|l| l.find('='))
}

/// The text with `logsink_project_id` bound with one space on each side of `=` — out
/// of the block's column, whatever alignment the fixture carries.
fn unaligned(text: &str) -> String {
    text.lines()
        .map(|l| {
            if l.trim_start().starts_with("logsink_project_id") {
                "  logsink_project_id = \"corp-log-infra-002\""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[tokio::test]
async fn a_replacement_alone_keeps_the_line_as_the_author_wrote_it() {
    let copy = support::copy_smoke();
    let session = copy.open("smoke.satz").await;
    std::fs::write(&session.main, unaligned(&support::read(&session.main))).unwrap();
    let ours = EditSession::open(&session.main)
        .unwrap()
        .apply(&[Edit::ReplaceParam {
            name: "logsink_project_id".to_string(),
            value: TypedValue::Str("corp-log-infra-003".to_string()),
        }])
        .unwrap();
    let report = support::interview(
        &session,
        serde_json::json!({"logsink_project_id": "corp-log-infra-003"}),
    )
    .await;
    assert_eq!(report.written, 1);
    let satz = support::read(&session.main);
    assert!(
        satz.contains("\n  logsink_project_id = \"corp-log-infra-003\"\n"),
        "{satz}"
    );
    assert_eq!(ours.text(), satz);
}

#[tokio::test]
async fn an_append_lays_the_block_out_and_the_two_write_the_same_bytes() {
    let copy = support::copy_smoke();
    let session = copy.open("smoke.satz").await;
    std::fs::write(&session.main, unaligned(&support::read(&session.main))).unwrap();
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
    // the line made unaligned above is back in the block's column, and the appended
    // answer, now the block's last entry, joins it
    let block = column(&satz, "customer_shortname");
    assert!(block.is_some(), "{satz}");
    assert_eq!(column(&satz, "logsink_project_id"), block, "{satz}");
    assert_eq!(column(&satz, "logsink_retention_days"), block, "{satz}");
    assert!(satz.contains(" = 400\n}\n"), "{satz}");
    assert_eq!(ours.text(), satz);
}

#[tokio::test]
async fn on_an_aligned_line_the_two_keep_the_column_and_write_the_same_bytes() {
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
    // the line the fixture aligns, wherever it puts it
    let line = before
        .iter()
        .position(|l| l.trim_start().starts_with("default_region"))
        .expect("the fixture binds default_region");
    for i in 0..before.len() {
        if i == line {
            continue;
        }
        assert_eq!(a[i], before[i], "line {}", i + 1);
        assert_eq!(b[i], before[i], "line {}", i + 1);
    }
    // the value is the only thing that moved: both keep the block's `=` column
    let head = before[line].split_once('=').unwrap().0;
    assert_eq!(a[line], format!("{head}= \"europe-west1\""));
    assert_eq!(b[line], a[line]);
    assert_eq!(ours.text(), satz);
}
