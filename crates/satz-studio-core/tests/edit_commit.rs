//! `Proposed::commit` and `Snapshot::verify` over a copy of the smoke estate: a no-op
//! lands and leaves the hash, a good edit lands through the MCP checker, a bad edit
//! rolls back naming the real file, a file changed on disk is refused, and a
//! delegated write is verified — or its bytes are put back.

#[path = "fixtures/edit/support.rs"]
mod support;

use satz_studio_core::cst::TypedValue;
use satz_studio_core::edit::{
    Checker, CommitError, Edit, EditSession, Rollback, Snapshot, sha256_hex,
};

fn num(n: &str) -> TypedValue {
    TypedValue::Num(n.to_string())
}

#[tokio::test]
async fn a_no_op_edit_commits_with_the_same_hash_through_both_checkers() {
    let copy = support::copy_smoke();
    let session = copy.open("showcase.satz").await;
    let (mcp, cli) = support::checkers(&session);
    let mut summaries = Vec::new();
    for checker in [&mcp as &dyn Checker, &cli as &dyn Checker] {
        let es = EditSession::open(&session.main).unwrap();
        let node = support::param_value(es.cst(), "audit_retention_days");
        let p = es
            .apply(&[Edit::ReplaceValue {
                node,
                value: num("400"),
            }])
            .unwrap();
        assert_eq!(p.text(), es.text());
        let c = support::within(p.commit(checker)).await.unwrap();
        assert_eq!(c.path, session.main);
        assert_eq!(c.sha256, es.sha256());
        assert_eq!(c.summary.estate, session.main.display().to_string());
        assert!(c.summary.written.is_empty());
        assert_eq!(support::read(&session.main), es.text());
        assert!(copy.temp_files().is_empty(), "{:?}", copy.temp_files());
        summaries.push(c.summary);
    }
    assert!(
        !summaries[0].addresses.is_empty(),
        "the MCP summary lists what the estate emits"
    );
    assert!(
        summaries[1].addresses.is_empty(),
        "the CLI prints no address list"
    );
}

#[tokio::test]
async fn a_good_edit_lands_through_the_mcp_checker() {
    let copy = support::copy_smoke();
    let session = copy.open("showcase.satz").await;
    let (mcp, _) = support::checkers(&session);
    let es = EditSession::open(&session.main).unwrap();
    let node = support::param_value(es.cst(), "audit_retention_days");
    let p = es
        .apply(&[Edit::ReplaceValue {
            node,
            value: num("30"),
        }])
        .unwrap();
    let c = support::within(p.commit(&mcp)).await.unwrap();
    let now = support::read(&session.main);
    assert!(
        now.contains("  audit_retention_days     = 30            # a number"),
        "{now}"
    );
    assert_eq!(c.sha256, sha256_hex(now.as_bytes()));
    assert_ne!(c.sha256, es.sha256());
    assert!(!c.summary.addresses.is_empty());
    assert!(copy.temp_files().is_empty(), "{:?}", copy.temp_files());
}

#[tokio::test]
async fn a_bad_edit_rolls_back_naming_the_real_file() {
    let copy = support::copy_smoke();
    let session = copy.open("showcase.satz").await;
    let (mcp, cli) = support::checkers(&session);
    for checker in [&mcp as &dyn Checker, &cli as &dyn Checker] {
        let es = EditSession::open(&session.main).unwrap();
        let node = support::attr_named(es.cst(), "location");
        let line = support::line_of(es.cst(), "location");
        let p = es
            .apply(&[Edit::ReplaceValue {
                node,
                value: TypedValue::Ref("nobody_declares_this".to_string()),
            }])
            .unwrap();
        let err = support::within(p.commit(checker)).await.unwrap_err();
        let CommitError::Rollback(Rollback::Check(diags)) = err else {
            panic!("{err}")
        };
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].file.as_deref(), Some(session.main.as_path()));
        assert_eq!(diags[0].line, Some(line));
        assert!(
            diags[0].message.contains("nobody_declares_this"),
            "{}",
            diags[0].message
        );
        assert_eq!(support::read(&session.main), es.text());
        assert!(copy.temp_files().is_empty(), "{:?}", copy.temp_files());
    }
}

#[tokio::test]
async fn a_file_changed_on_disk_is_refused() {
    let copy = support::copy_smoke();
    let session = copy.open("showcase.satz").await;
    let (mcp, _) = support::checkers(&session);
    let es = EditSession::open(&session.main).unwrap();
    let node = support::param_value(es.cst(), "audit_retention_days");
    let p = es
        .apply(&[Edit::ReplaceValue {
            node,
            value: num("30"),
        }])
        .unwrap();
    let elsewhere = es.text().replace("\"corp-IaC\"", "\"corp-iac-2\"");
    assert_ne!(elsewhere, es.text());
    std::fs::write(&session.main, &elsewhere).unwrap();
    let err = support::within(p.commit(&mcp)).await.unwrap_err();
    assert!(
        matches!(err, CommitError::Rollback(Rollback::ChangedOnDisk)),
        "{err}"
    );
    assert_eq!(support::read(&session.main), elsewhere);
    assert!(copy.temp_files().is_empty(), "{:?}", copy.temp_files());
}

#[tokio::test]
async fn a_delegated_write_is_verified_and_a_broken_one_is_restored() {
    let copy = support::copy_smoke();
    let session = copy.open("smoke.satz").await;
    let (mcp, cli) = support::checkers(&session);

    let snapshot = Snapshot::take(&session.main).unwrap();
    assert_eq!(snapshot.path(), session.main);
    assert_eq!(snapshot.sha256(), sha256_hex(snapshot.bytes()));
    let report =
        support::interview(&session, serde_json::json!({"logsink_retention_days": 400})).await;
    assert_eq!(report.written, 1);
    let c = support::within(snapshot.verify(&mcp)).await.unwrap();
    let now = support::read(&session.main);
    assert!(now.contains("\n  logsink_retention_days = 400\n"), "{now}");
    assert_eq!(c.sha256, sha256_hex(now.as_bytes()));
    assert_eq!(c.path, session.main);
    assert!(!c.summary.addresses.is_empty());

    // a delegated write that breaks the estate: the check refuses it and the bytes go back
    let snapshot = Snapshot::take(&session.main).unwrap();
    assert_eq!(snapshot.bytes(), now.as_bytes());
    // by key, not by layout: the fixture aligns its params, and may re-align them
    let broken: String = now
        .lines()
        .map(|l| {
            if l.trim_start().starts_with("default_region") {
                "  default_region = nobody_declares_this"
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_ne!(broken, now);
    std::fs::write(&session.main, &broken).unwrap();
    let err = support::within(snapshot.verify(&cli)).await.unwrap_err();
    let CommitError::Rollback(Rollback::Check(diags)) = err else {
        panic!("{err}")
    };
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].file.as_deref(), Some(session.main.as_path()));
    assert_eq!(diags[0].line, Some(24));
    assert_eq!(
        support::read(&session.main),
        now,
        "the snapshot's bytes are back"
    );
    assert!(copy.temp_files().is_empty());
}
