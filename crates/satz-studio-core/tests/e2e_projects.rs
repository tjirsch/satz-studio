//! The Interfaces tab and its wizard over the real satz, on a copy of satz's smoke
//! showcase: `satz interfaces` read through the CLI as the reload reads it, and
//! `satz add-project` run through the path the app's `AddProject` takes — the write lock,
//! the bytes recorded, the command inside `Snapshot::delegate` with `McpChecker`
//! (`edit::delegated_write`). An interface that lands is in the file and in the next
//! report, and passes satz's check; one satz refuses leaves the file byte-identical.

#[path = "fixtures/e2e/support.rs"]
mod e2e;
#[path = "fixtures/edit/support.rs"]
mod support;

use satz_studio_core::edit::{self, Cause, Delegated, Restore};
use satz_studio_core::satz::project::{self, AddProjectArgs};
use satz_studio_core::satz::reports::{ExportHow, FindingSeverity};

#[tokio::test]
async fn the_report_reads_and_an_interface_is_added_or_refused() {
    let copy = support::copy_smoke();
    let session = copy.open("showcase.satz").await;

    // what the estate publishes, through the CLI
    let report = e2e::within(project::interfaces(&session.cli, &session.main))
        .await
        .unwrap();
    let audit = report
        .interfaces
        .iter()
        .find(|i| i.name == "audit")
        .expect("the showcase declares `audit`");
    assert!(audit.common, "{audit:?}");
    let archive = report
        .interfaces
        .iter()
        .find(|i| i.name == "archive")
        .expect("the showcase declares `archive`");
    assert!(!archive.common);
    assert_eq!(archive.uses, ["audit"]);
    let id = report
        .of("archive")
        .find(|e| e.name == "archive_project_id")
        .expect("archive exports archive_project_id");
    // the list a project may add entries to, and the shape of an entry
    let topics = report
        .requests
        .iter()
        .find(|r| r.param == "event_topics")
        .expect("the showcase takes requests for event_topics");
    assert_eq!(topics.key, "name");
    assert_eq!(topics.fields, ["name", "retention"]);
    assert_eq!(id.attach, ["google_project_iam_member"]);
    assert!(
        report.core().any(|e| e.name == "workload_folder"),
        "a core export"
    );
    let map = report
        .exports
        .iter()
        .find(|e| e.how == ExportHow::Map)
        .expect("a map export");
    assert!(map.all.is_some(), "{map:?}");
    assert!(report.interfaces.iter().all(|i| i.name != "reports"));

    // an interface alone, using `audit` and carrying one of archive's exports again
    let args = AddProjectArgs {
        name: "reports".to_string(),
        interface_only: true,
        uses: vec!["audit".to_string()],
        exports: vec!["archive.archive_project_number".to_string()],
        ..Default::default()
    };
    assert_eq!(args.problem(), None);
    let before = support::read(&session.main);
    let written = e2e::within(edit::delegated_write(
        &session,
        project::add_project(&session.cli, &session.main, &args),
    ))
    .await
    .unwrap();
    let Delegated::Landed { outcome, committed } = written else {
        panic!("add-project did not land: {written:?}")
    };
    assert!(!outcome.is_error);
    assert!(
        outcome.text.contains("add-project reports"),
        "{}",
        outcome.text
    );
    // landed is the check's pass on the real path; what it reported is no error
    assert!(
        committed
            .summary
            .findings
            .iter()
            .all(|f| f.severity != FindingSeverity::Error),
        "{:?}",
        committed.summary
    );
    let after = support::read(&session.main);
    assert!(
        after.starts_with(before.trim_end()),
        "satz appends at the end"
    );
    let block = &after[after.find("interface \"reports\" {").expect("the block")..];
    let block = &block[..block.find("\n}").unwrap()];
    assert!(block.contains("use interface \"audit\""), "{block}");
    assert!(
        block
            .contains("export \"archive_project_number\" = \"${{google_project.archive.number}}\""),
        "{block}"
    );
    assert!(copy.temp_files().is_empty());
    let again = e2e::within(project::interfaces(&session.cli, &session.main))
        .await
        .unwrap();
    let reports = again
        .interfaces
        .iter()
        .find(|i| i.name == "reports")
        .expect("the next report names it");
    assert_eq!(reports.uses, ["audit"]);
    assert!(
        again
            .of("reports")
            .any(|e| e.name == "archive_project_number")
    );

    // a name the estate declares already: satz refuses and the file stays as it was
    let twice = AddProjectArgs {
        name: "archive".to_string(),
        ..args
    };
    assert_eq!(
        twice.problem(),
        None,
        "only the file knows the name is taken"
    );
    let written = e2e::within(edit::delegated_write(
        &session,
        project::add_project(&session.cli, &session.main, &twice),
    ))
    .await
    .unwrap();
    let Delegated::NotLanded(not_landed) = written else {
        panic!("a second `archive` landed: {written:?}")
    };
    let Cause::Refused(outcome) = &not_landed.cause else {
        panic!("{:?}", not_landed.cause)
    };
    assert!(
        outcome
            .text
            .contains("declares `interface \"archive\"` already"),
        "{}",
        outcome.text
    );
    assert!(!outcome.text.contains("satz v"), "the banner is dropped");
    assert!(matches!(not_landed.restore, Restore::Untouched));
    assert_eq!(support::read(&session.main), after);
}
