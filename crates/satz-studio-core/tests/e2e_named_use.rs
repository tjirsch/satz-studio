//! A named use over the real satz (satz's ADR 0109): one pack used as `web`, as `db` and
//! once flat, the corpus case `vendor/satz/tests/corpus/named-uses/` copied into an estate
//! with `web.vm_zone` left unbound. The questions are asked once per use and grouped as
//! satz's interview groups them; an answer keyed by the dotted subject is written as that
//! binding by satz's own writer; the model reads the `named` line and puts the dotted
//! params under their use. On satz's smoke showcase, which uses `showcase-bucket.satz` as
//! `reports` and `scratch`, an entry is added to a use's own list through
//! `satz_add_entry`, by the name the estate binds it.

#[path = "fixtures/e2e/support.rs"]
mod e2e;
#[path = "fixtures/edit/support.rs"]
mod support;

use std::collections::BTreeMap;

use satz_studio_core::cst::Cst;
use satz_studio_core::edit::Delegated;
use satz_studio_core::satz::project;
use satz_studio_core::satz::reports::{AddEntryArgs, QuestionState};

const ESTATE: &str = "named.satz";

/// The corpus case in an estate of its own, `web.vm_zone` taken out of its params.
fn named_estate() -> e2e::Estate {
    let estate = e2e::estate_dir(None);
    let corpus = e2e::vendor().join("tests/corpus/named-uses");
    std::fs::copy(corpus.join("vm.satz"), estate.yaml.join("vm.satz")).unwrap();
    let main = std::fs::read_to_string(corpus.join("main.satz")).unwrap();
    let main: String = main
        .lines()
        .filter(|l| !l.trim_start().starts_with("web.vm_zone"))
        .map(|l| format!("{l}\n"))
        .collect();
    std::fs::write(estate.yaml.join(ESTATE), main).unwrap();
    estate
}

#[tokio::test]
async fn a_named_use_is_asked_grouped_answered_dotted_and_read_by_the_model() {
    let estate = named_estate();
    let session = estate.open(ESTATE).await;

    let report = e2e::questions(&session).await;
    let zone = |named: Option<&str>| {
        report
            .questions
            .iter()
            .find(|q| q.named.as_deref() == named && q.label() == "vm_zone")
            .unwrap_or_else(|| panic!("vm_zone for {named:?}: {:#?}", report.questions))
    };
    let web = zone(Some("web"));
    assert_eq!(web.subject, "web.vm_zone");
    assert_eq!(web.param.as_deref(), Some("vm_zone"));
    assert_eq!(web.state, QuestionState::Unanswered);
    assert_eq!(web.group(), "web — vm.satz");
    let db = zone(Some("db"));
    assert_eq!(db.subject, "db.vm_zone");
    assert_eq!(db.state, QuestionState::Answered);
    assert_eq!(db.group(), "db — vm.satz");
    // the flat use asks its own, under the pack
    let flat = zone(None);
    assert_eq!(flat.subject, "vm_zone");
    assert_eq!(flat.group(), flat.pack);
    // each use's questions stand together, as satz's interview asks them
    let groups: Vec<String> = report.questions.iter().map(|q| q.group()).collect();
    let mut seen: Vec<&String> = Vec::new();
    for (i, g) in groups.iter().enumerate() {
        if i == 0 || groups[i - 1] != *g {
            assert!(!seen.contains(&g), "the group {g} stands twice: {groups:?}");
            seen.push(g);
        }
    }

    // the answer is keyed by the dotted subject and written as that binding
    let (written, _) = e2e::answer(
        &session,
        e2e::one_answer("web.vm_zone", serde_json::json!("europe-west3-c")),
    )
    .await;
    assert_eq!(written.written, 1);
    let text = std::fs::read_to_string(&session.main).unwrap();
    let params = e2e::params_of(&Cst::parse(&text).unwrap());
    assert_eq!(
        params.get("web.vm_zone").map(String::as_str),
        Some("\"europe-west3-c\"")
    );
    assert_eq!(
        params.get("db.vm_zone").map(String::as_str),
        Some("\"europe-west3-b\""),
        "the other use's binding is untouched"
    );
    let after = e2e::questions(&session).await;
    assert!(
        after
            .questions
            .iter()
            .filter(|q| q.subject == "web.vm_zone")
            .all(|q| q.state == QuestionState::Answered)
    );

    // the model: the named lines read, each use's params under it
    let model = e2e::model(&session, Vec::new()).await;
    let names: BTreeMap<&str, Option<&str>> = model
        .params
        .iter()
        .map(|r| (r.name.as_str(), r.named.as_ref().map(|u| u.name.as_str())))
        .collect();
    assert_eq!(names["web.vm_zone"], Some("web"));
    assert_eq!(names["db.vm_machine_type"], Some("db"));
    assert_eq!(names["vm_name"], None);
    let web_zone = model
        .params
        .iter()
        .find(|r| r.name == "web.vm_zone")
        .unwrap();
    assert_eq!(web_zone.own_name(), "vm_zone");
    assert_eq!(
        web_zone.named.as_ref().map(|u| u.heading()).as_deref(),
        Some("web — vm.satz")
    );
    assert_eq!(
        web_zone.question.as_ref().map(|q| q.subject.as_str()),
        Some("web.vm_zone"),
        "the row is joined with the use's question"
    );
}

/// `satz_add_entry` on a named use's list: the list is named as the estate binds it,
/// `reports.bucket_archives`, and the entry lands in the estate's `reports.bucket_archives
/// = [ … ]`, which `satz interfaces` then reports as that use's.
#[tokio::test]
async fn an_entry_is_added_to_a_named_use_s_list() {
    let copy = support::copy_smoke();
    let session = copy.open("showcase.satz").await;
    let report = e2e::within(project::interfaces(&session.cli, &session.main))
        .await
        .unwrap();
    let list = report
        .list_params
        .iter()
        .find(|r| r.param == "reports.bucket_archives")
        .unwrap_or_else(|| panic!("the use's list: {:#?}", report.list_params));
    assert_eq!(list.named.as_deref(), Some("reports"));
    assert_eq!(list.entries, 0);
    assert!(
        report
            .list_params
            .iter()
            .filter(|r| !r.param.contains('.'))
            .all(|r| r.named.is_none()),
        "a flat list belongs to no use"
    );

    let args = AddEntryArgs {
        list: list.param.clone(),
        fields: BTreeMap::from([("name".to_string(), "old".to_string())]),
    };
    match e2e::delegate(&session, "satz_add_entry", &args).await {
        Delegated::Landed { .. } => {}
        Delegated::RolledBack { error, .. } => panic!("satz_add_entry rolled back: {error}"),
        Delegated::NotLanded(n) => panic!("{}", n.message("satz_add_entry")),
    }
    let text = std::fs::read_to_string(&session.main).unwrap();
    let params = e2e::params_of(&Cst::parse(&text).unwrap());
    let bound = params
        .get("reports.bucket_archives")
        .expect("the use's list is bound in the estate");
    assert!(bound.contains("\"old\""), "{bound}");
    assert!(!params.contains_key("scratch.bucket_archives"));

    let after = e2e::within(project::interfaces(&session.cli, &session.main))
        .await
        .unwrap();
    let list = after
        .list_params
        .iter()
        .find(|r| r.param == "reports.bucket_archives")
        .unwrap();
    assert_eq!(list.entries, 1);
}
