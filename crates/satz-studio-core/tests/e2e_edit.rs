//! The app's own writer on disk, over a copy of the smoke estate: a `ReplaceValue`
//! with the node's own value leaves the file's hash and the output of
//! `satz transpile --check` unchanged; an edit replaces the value span and nothing
//! else — the `=` column and the trailing comment keep their bytes — and the reverse
//! edit restores the original bytes. What `edit_commit.rs` proves already (a good
//! edit lands, a bad one rolls back naming the real file, a file changed on disk is
//! refused, a delegated write is verified or restored) is not repeated here.

#[path = "fixtures/e2e/support.rs"]
mod e2e;
#[path = "fixtures/edit/support.rs"]
mod support;

use satz_studio_core::cst::{NodeKind, TypedValue};
use satz_studio_core::edit::{Edit, EditSession, sha256_hex};

const PARAM: &str = "audit_retention_days";

#[tokio::test]
async fn a_no_op_edit_leaves_the_hash_and_the_check_output_unchanged() {
    let copy = support::copy_smoke();
    let session = copy.open("showcase.satz").await;
    let (_, cli) = support::checkers(&session);

    let (passed, before) = e2e::check_lines(&session.cli, &session.main).await;
    assert!(passed, "{before:?}");
    assert!(!before.is_empty());

    let es = EditSession::open(&session.main).unwrap();
    let node = support::param_value(es.cst(), PARAM);
    let own = es.cst().slice(es.cst().node(node).span).to_string();
    assert_eq!(own, "400");
    let proposed = es
        .apply(&[Edit::ReplaceValue {
            node,
            value: TypedValue::Num(own),
        }])
        .unwrap();
    assert_eq!(proposed.text(), es.text());
    let committed = support::within(proposed.commit(&cli)).await.unwrap();
    assert_eq!(committed.sha256, es.sha256());
    assert_eq!(support::read(&session.main), es.text());
    assert!(copy.temp_files().is_empty(), "{:?}", copy.temp_files());

    let (passed, after) = e2e::check_lines(&session.cli, &session.main).await;
    assert!(passed, "{after:?}");
    assert_eq!(before, after, "the check says the same, line for line");
}

#[tokio::test]
async fn an_edit_replaces_the_value_span_alone_and_the_reverse_edit_restores_the_bytes() {
    let copy = support::copy_smoke();
    let session = copy.open("showcase.satz").await;
    let (mcp, _) = support::checkers(&session);

    let es = EditSession::open(&session.main).unwrap();
    let original = es.text().to_string();
    let original_sha = es.sha256().to_string();
    let entry = es.cst().param(PARAM).unwrap();
    let NodeKind::ParamEntry { value, .. } = es.cst().node(entry).kind else {
        panic!("{:?}", es.cst().node(entry).kind)
    };
    let line = es.cst().node(entry).line as usize;
    assert_eq!(line, support::line_of(es.cst(), PARAM) as usize);
    let value_span = es.cst().node(value).span;

    let proposed = es
        .apply(&[Edit::ReplaceValue {
            node: entry,
            value: TypedValue::Num("30".to_string()),
        }])
        .unwrap();
    let committed = support::within(proposed.commit(&mcp)).await.unwrap();
    let edited = support::read(&session.main);
    assert_eq!(committed.sha256, sha256_hex(edited.as_bytes()));
    assert_ne!(edited, original);

    // the value span and nothing else: the bytes before it and after it are the original's
    assert_eq!(&edited[..value_span.start], &original[..value_span.start]);
    assert_eq!(
        &edited[value_span.start + 2..],
        &original[value_span.end..],
        "the bytes after the value — its spacing and the trailing comment — are the original's"
    );
    assert_eq!(&edited[value_span.start..value_span.start + 2], "30");

    // on the line: the `=` column, and the comment with its spacing
    let before: Vec<&str> = original.lines().collect();
    let after: Vec<&str> = edited.lines().collect();
    assert_eq!(before.len(), after.len());
    for (i, (a, b)) in before.iter().zip(&after).enumerate() {
        if i + 1 != line {
            assert_eq!(a, b, "line {}", i + 1);
        }
    }
    let (old_head, old_rest) = before[line - 1].split_once("= ").unwrap();
    let (new_head, new_rest) = after[line - 1].split_once("= ").unwrap();
    assert_eq!(old_head, new_head, "the `=` column");
    assert!(old_rest.starts_with("400"), "{old_rest}");
    assert_eq!(
        new_rest.strip_prefix("30").unwrap(),
        old_rest.strip_prefix("400").unwrap(),
        "the trailing comment and the spaces before it"
    );
    assert!(new_rest.contains("# a number"), "{new_rest}");

    // the reverse edit restores the original bytes and hash
    let es = EditSession::open(&session.main).unwrap();
    let node = support::param_value(es.cst(), PARAM);
    let proposed = es
        .apply(&[Edit::ReplaceValue {
            node,
            value: TypedValue::Num("400".to_string()),
        }])
        .unwrap();
    let committed = support::within(proposed.commit(&mcp)).await.unwrap();
    assert_eq!(support::read(&session.main), original);
    assert_eq!(committed.sha256, original_sha);
    assert!(copy.temp_files().is_empty(), "{:?}", copy.temp_files());
}
