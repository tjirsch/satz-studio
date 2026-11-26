//! The Interfaces tab over the real satz, on a copy of satz's smoke showcase: `satz
//! interfaces` read through the CLI as the reload reads it — the core exports, the
//! declared interfaces, one interface per entry of a list that a top-level `each` writes
//! (satz ADR 0075), the interface of each named use of a pack that declares one (satz ADR
//! 0109), and the list params with the pattern each field's value matches.

#[path = "fixtures/e2e/support.rs"]
mod e2e;
#[path = "fixtures/edit/support.rs"]
mod support;

use satz_studio_core::satz::project;
use satz_studio_core::satz::reports::ExportHow;

#[tokio::test]
async fn the_report_reads_what_the_estate_publishes_and_its_list_params() {
    let copy = support::copy_smoke();
    let session = copy.open("showcase.satz").await;

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
    assert_eq!(id.attach, ["google_project_iam_member"]);
    assert!(
        report.core().any(|e| e.name == "infra_folder"),
        "a core export"
    );
    let map = report
        .exports
        .iter()
        .find(|e| e.how == ExportHow::Map)
        .expect("a map export");
    assert!(map.all.is_some(), "{map:?}");

    // `each event_topics by name { interface "{each.name}-events" { … } }`: one interface
    // per entry, each a project's own, using `audit` and exporting its own topic
    for name in ["orders-events", "billing-events"] {
        let iface = report
            .interfaces
            .iter()
            .find(|i| i.name == name)
            .unwrap_or_else(|| panic!("the showcase's each writes `{name}`"));
        assert!(!iface.common, "{iface:?}");
        assert_eq!(iface.uses, ["audit"]);
        let topic = report
            .of(name)
            .find(|e| e.name == "topic")
            .unwrap_or_else(|| panic!("`{name}` exports its topic"));
        assert!(
            topic.value.contains(&name.replace("-events", "")),
            "{topic:?}"
        );
        assert_eq!(iface.named, None);
    }

    // `use "showcase-bucket.satz" named reports` and `… named scratch`: the pack's
    // `interface "bucket"` is each use's own, `<use>-bucket`, never common (satz 0.119.0)
    for (name, of_use) in [("reports-bucket", "reports"), ("scratch-bucket", "scratch")] {
        let iface = report
            .interfaces
            .iter()
            .find(|i| i.name == name)
            .unwrap_or_else(|| panic!("the named use `{of_use}` publishes `{name}`"));
        assert_eq!(iface.named.as_deref(), Some(of_use), "{iface:?}");
        assert!(!iface.common, "{iface:?}");
        assert!(
            report.of(name).any(|e| e.name == "bucket_name"),
            "`{name}` exports its bucket's name"
        );
    }

    // the list a project may add entries to, the shape of an entry, and the pattern the
    // whole of each field's value matches
    let topics = report
        .list_params
        .iter()
        .find(|r| r.param == "event_topics")
        .expect("the showcase declares the list param event_topics");
    assert_eq!(topics.key, "name");
    assert_eq!(
        topics.fields,
        ["name", "retention", "subscription", "dead_letter"]
    );
    // a field whose default is another field of the same entry
    assert_eq!(
        topics.field_defaults.get("dead_letter").map(String::as_str),
        Some("name")
    );
    assert_eq!(
        topics.patterns.get("name").map(String::as_str),
        Some("[a-z][a-z0-9-]*")
    );
    assert_eq!(
        topics.patterns.get("retention").map(String::as_str),
        Some("[0-9]+s")
    );
    assert_eq!(
        topics.fields_shown(),
        "name ~ [a-z][a-z0-9-]*, retention ~ [0-9]+s, subscription, dead_letter ~ [a-z][a-z0-9-]*"
    );
}
