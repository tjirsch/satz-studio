//! The Packs view's switches, driven as the view drives them, on the interviewed
//! skeleton: the map switched on through `satz_add_pack`, which uncomments its line; a
//! pack switched on, which binds its gate and makes its line active, and off, which binds
//! the gate false and leaves the line; a switch satz refuses, which writes nothing and
//! names why; and a gate bound true whose line is gone, which satz's pack report shows as
//! an absent pack with the finding and the command that answers it, and which
//! `satz transpile --check` names — a warning at satz's default validation level, a
//! refusal at `error`. And a gate answered through `satz_interview` that satz refuses
//! after it has written the file: the file is back as it was either way, and the refusal
//! says it was put back exactly when it was. A pack's line stripped of its ` when <gate>`
//! is the satz→studio contract the Packs view's chip and sentence stand on: the row reads
//! `ungated`, the report carries the `ungated-pack` finding, and the check names it.

#[path = "fixtures/e2e/support.rs"]
mod support;

use satz_studio_core::cst::{Cst, TypedValue, UseState, scan_uses};
use satz_studio_core::diag::{DiagSource, Severity, parse_satz_output};
use satz_studio_core::edit::{
    Cause, CheckFailure, Checker, CliChecker, CommitError, Delegated, Edit, EditSession,
    McpChecker, Restore, Rollback, sha256_hex,
};
use satz_studio_core::satz::reports::{AddPackArgs, PackLine, QuestionState};
use std::sync::Arc;

const BILLING: &str = "presets/billing-account-permissions.satz";
const BUDGET: &str = "presets/organization-budget.satz";

#[tokio::test]
async fn the_map_and_a_pack_are_switched_by_satz_and_a_refused_switch_writes_nothing() {
    let estate = support::estate_dir(None);
    let main = estate.create_skeleton("new.satz").await;
    let session = estate.open("new.satz").await;
    support::answer_like_the_smoke_matrix(&session).await;

    let before = support::read(&main);
    let map_line = "// use \"presets/estate-map.satz\"\n";
    assert!(before.contains(map_line), "{before}");
    let m = support::model(&session, Vec::new()).await;
    let map = m.packs.map().expect("the graph has the map");
    assert_eq!(map.line, PackLine::Commented);
    assert!(!map.deploys);

    let (change, committed) = support::add_pack(&session, support::MAP).await;
    assert_eq!(change.switched, [support::MAP]);
    assert!(change.bound.is_empty(), "the map has no gate: {change:?}");
    let text = support::read(&main);
    assert_eq!(
        text,
        before.replacen(map_line, "use \"presets/estate-map.satz\"\n", 1),
        "the `// ` goes and nothing else changes"
    );
    assert_eq!(committed.sha256, sha256_hex(text.as_bytes()));
    assert!(!committed.summary.addresses.is_empty());
    let m = support::model(&session, Vec::new()).await;
    let map = m.packs.map().unwrap();
    assert_eq!((map.line, map.deploys), (PackLine::Active, true));

    // with the map in, its choices are open
    let report = support::questions(&session).await;
    assert!(report.summary.total > 16, "{:?}", report.summary);
    let billing = report
        .questions
        .iter()
        .find(|q| q.subject == "use_billing_permissions")
        .expect("the map asks about the billing permissions");
    assert_eq!(billing.state, QuestionState::Unanswered);

    // the billing permissions need a security-group model, and none is on: refused,
    // naming the packs that would meet it, the file as it was
    let refused = support::switch(
        &session,
        "satz_add_pack",
        &AddPackArgs {
            pack: BILLING.to_string(),
            with_requirements: false,
        },
    )
    .await
    .expect_err("billing without a security model");
    assert!(refused.contains("refused, nothing written"), "{refused}");
    assert!(refused.contains("security-group"), "{refused}");
    assert_eq!(support::read(&main), text);

    // on: satz binds the gate and uncomments the line
    let commented = format!("\n// use \"{BUDGET}\" when use_budget\n");
    let active = format!("\nuse \"{BUDGET}\" when use_budget\n");
    assert!(text.contains(&commented), "{text}");
    let (change, _) = support::add_pack(&session, BUDGET).await;
    assert_eq!(change.switched, [BUDGET]);
    let text = support::read(&main);
    assert!(text.contains(&active), "{text}");
    assert!(!text.contains(&commented));
    // by param, not by spacing: satz's writer keeps a formatted file formatted
    let cst = Cst::parse(&text).unwrap();
    assert_eq!(
        support::params_of(&cst)
            .get("use_budget")
            .map(String::as_str),
        Some("true"),
        "{text}"
    );
    let line = scan_uses(&cst)
        .into_iter()
        .find(|u| u.path == BUDGET)
        .unwrap();
    assert_eq!(line.state, UseState::Active);
    let m = support::model(&session, Vec::new()).await;
    let row = m.packs.row(BUDGET).unwrap();
    assert_eq!((row.line, row.deploys), (PackLine::Active, true));
    assert_eq!(row.at_line, Some(line.line));
    assert_eq!(row.value, Some(true));
    assert!(m.params.iter().all(|p| p.name != "use_budget"));

    // off: the gate reads false and the line stays active — a gated line with a false
    // gate deploys nothing
    let (change, _) = support::remove_pack(&session, BUDGET).await;
    assert_eq!(change.switched, [BUDGET]);
    let text = support::read(&main);
    assert!(text.contains(&active), "{text}");
    assert_eq!(
        support::params_of(&Cst::parse(&text).unwrap())
            .get("use_budget")
            .map(String::as_str),
        Some("false"),
        "{text}"
    );
    let m = support::model(&session, Vec::new()).await;
    let row = m.packs.row(BUDGET).unwrap();
    assert_eq!((row.line, row.deploys), (PackLine::Active, false));
    assert_eq!(row.value, Some(false));
    assert!(estate.temp_files().is_empty(), "{:?}", estate.temp_files());
}

#[tokio::test]
async fn a_gate_bound_true_without_its_line_is_absent_and_the_check_names_the_pack() {
    let estate = support::estate_dir(None);
    let main = estate.create_skeleton("new.satz").await;
    let session = estate.open("new.satz").await;
    support::answer_like_the_smoke_matrix(&session).await;
    support::add_pack(&session, support::MAP).await;

    // the budget pack goes back to how it looks in an estate written before the
    // library gained it: no line for it, active or commented, and no binding of its gate
    let text = support::read(&main);
    assert!(text.contains(BUDGET), "{text}");
    let without: String = text
        .lines()
        .filter(|l| !l.contains(BUDGET) && !l.trim_start().starts_with("use_budget "))
        .map(|l| format!("{l}\n"))
        .collect();
    std::fs::write(&main, &without).unwrap();

    // `use_budget = true` the app's way: appended by `ReplaceParam`, and the commit
    // lands — at satz's default validation level the check passes
    let mcp = McpChecker {
        session: Arc::clone(&session),
    };
    let es = EditSession::open(&main).unwrap();
    let proposed = es
        .apply(&[Edit::ReplaceParam {
            name: "use_budget".to_string(),
            value: TypedValue::Bool(true),
        }])
        .unwrap();
    let committed = support::within(proposed.commit(&mcp)).await.unwrap();
    let text = support::read(&main);
    // the block's last entry, laid out in its `=` column as satz's `bind` appends
    let last = text
        .split("\n}\n")
        .next()
        .and_then(|params| params.lines().last())
        .unwrap_or_default();
    assert_eq!(
        last.split_once('=')
            .map(|(name, value)| (name.trim(), value.trim())),
        Some(("use_budget", "true")),
        "{text}"
    );
    let shortname = text
        .lines()
        .find(|l| l.trim_start().starts_with("customer_shortname "))
        .and_then(|l| l.find('='));
    assert_eq!(last.find('='), shortname, "{text}");
    assert_eq!(committed.sha256, sha256_hex(text.as_bytes()));

    // and says so: a warning on stderr, which parses to a diagnostic naming the pack
    // and the remedy
    let (passed, lines) = support::check_lines(&session.cli, &main).await;
    assert!(passed, "{lines:?}");
    let diags = parse_satz_output(&support::stderr_of(&lines), DiagSource::Check);
    let warning = diags
        .iter()
        .find(|d| d.message.contains("asks for but does not use"))
        .unwrap_or_else(|| panic!("no unadopted-pack warning in {diags:?}"));
    assert_eq!(warning.severity, Severity::Warning);
    let sentence = format!("`use_budget` is true and this estate has no line for `{BUDGET}`");
    assert!(warning.message.contains(&sentence), "{}", warning.message);
    assert!(
        warning
            .message
            .contains(&format!("fix: satz add-pack new.satz {BUDGET}")),
        "{}",
        warning.message
    );

    // the pack report: the pack is absent, its gate true, the finding on its row, and
    // the command that answers it in the report's findings; the gate is no param row
    let m = support::model(&session, diags.clone()).await;
    let row = m.packs.row(BUDGET).unwrap();
    assert_eq!(
        (row.line, row.at_line, row.deploys),
        (PackLine::Absent, None, false)
    );
    assert_eq!(row.answer.as_deref(), Some("true"));
    assert!(
        row.findings.iter().any(|f| f.contains(&sentence)),
        "{:?}",
        row.findings
    );
    let finding = m
        .packs
        .findings
        .iter()
        .find(|f| f.subject.as_deref() == Some(BUDGET))
        .unwrap_or_else(|| panic!("no finding about {BUDGET}: {:?}", m.packs.findings));
    assert_eq!(finding.kind, "unadopted-pack");
    assert!(
        finding
            .fix
            .as_deref()
            .is_some_and(|f| f.starts_with("satz add-pack ") && f.ends_with(BUDGET)),
        "{finding:?}"
    );
    assert!(m.params.iter().all(|p| p.name != "use_budget"));
    assert!(m.diagnostics.contains(warning), "{:?}", m.diagnostics);

    // at validation level `error` the same sentence is a refusal: the CLI checker over
    // a config at that level returns it as the diagnostic …
    let strict_dir = support::config_over(&estate.yaml, "error");
    let strict = CliChecker {
        cli: strict_dir.cli().await,
    };
    let err = support::within(strict.check(&main)).await.unwrap_err();
    let CheckFailure::Refused(refused) = err else {
        panic!("{err:?}")
    };
    // the pack whose line is gone, among the packs this estate asks for and does not
    // use; how many others share that group is the library's business, not this test's
    let budget = refused
        .iter()
        .find(|d| d.message.contains("`use_budget` is true"))
        .unwrap_or_else(|| panic!("no refusal names use_budget: {refused:?}"));
    assert_eq!(budget.severity, Severity::Error);
    assert_eq!(budget.source, DiagSource::Check);
    assert_eq!(budget.kind.as_deref(), Some("unadopted-pack"));
    assert!(
        budget
            .message
            .contains("packs this estate asks for but does not use"),
        "{}",
        budget.message
    );
    assert!(
        budget.message.contains(&format!(
            "{sentence} — the command writes it where the pack graph places it"
        )),
        "{}",
        budget.message
    );

    // … and the write discipline rolls a binding back: switching the pack off lands
    // through the estate's own checker, where a pack nothing emits is a warning;
    // switching it on again is refused by the strict one with that diagnostic, the
    // file keeps its bytes, no temp file stays
    let es = EditSession::open(&main).unwrap();
    let proposed = es
        .apply(&[Edit::ReplaceParam {
            name: "use_budget".to_string(),
            value: TypedValue::Bool(false),
        }])
        .unwrap();
    support::within(proposed.commit(&mcp)).await.unwrap();
    let off = support::read(&main);
    assert_eq!(
        off.lines()
            .find(|l| l.trim_start().starts_with("use_budget "))
            .and_then(|l| l.split_once('='))
            .map(|(_, value)| value.trim()),
        Some("false"),
        "{off}"
    );
    let es = EditSession::open(&main).unwrap();
    let proposed = es
        .apply(&[Edit::ReplaceParam {
            name: "use_budget".to_string(),
            value: TypedValue::Bool(true),
        }])
        .unwrap();
    let err = support::within(proposed.commit(&strict)).await.unwrap_err();
    let CommitError::Rollback(Rollback::Check(diags)) = err else {
        panic!("{err}")
    };
    assert_eq!(diags, refused);
    assert_eq!(support::read(&main), off);

    // the finding's command, as the Packs view's switch runs it, writes the line where
    // the pack graph places it
    support::add_pack(&session, BUDGET).await;
    let m = support::model(&session, Vec::new()).await;
    let row = m.packs.row(BUDGET).unwrap();
    assert_eq!((row.line, row.deploys), (PackLine::Active, true));
    assert!(row.findings.is_empty(), "{:?}", row.findings);
    assert!(estate.temp_files().is_empty(), "{:?}", estate.temp_files());
}

#[tokio::test]
async fn a_refused_answer_leaves_the_file_as_it_was_and_says_so_when_satz_had_changed_it() {
    let estate = support::estate_dir(None);
    let main = estate.create_skeleton("new.satz").await;
    let session = estate.open("new.satz").await;
    support::answer_like_the_smoke_matrix(&session).await;
    let (change, _) = support::switch(
        &session,
        "satz_add_pack",
        &AddPackArgs {
            pack: support::MAP.to_string(),
            with_requirements: true,
        },
    )
    .await
    .unwrap_or_else(|e| panic!("add-pack the map: {e}"));
    assert_eq!(change.switched, [support::MAP]);

    // the central alerts read the audit logsink's project, and the logsink is off: the
    // answer does not compile. satz 0.73.1 binds the gate and uncomments the line before
    // it refuses; a satz that refuses first writes nothing. Either way the file is what it
    // was, and the refusal names a restore exactly when there was one.
    let before = support::read(&main);
    let alerts = "// use \"presets/monitoring/organization-cis-log-alerts-central.satz\" when use_central_alerts";
    assert!(before.contains(alerts), "{before}");
    let delegated = support::delegate(
        &session,
        "satz_interview",
        &support::one_answer("use_central_alerts", serde_json::json!(true)),
    )
    .await;
    let Delegated::NotLanded(refused) = delegated else {
        panic!("the answer landed: {delegated:?}")
    };
    let Cause::Refused(outcome) = &refused.cause else {
        panic!("{refused:?}")
    };
    assert!(outcome.text.contains("logsink"), "{}", outcome.text);
    assert_eq!(support::read(&main), before, "the file is back as it was");
    let message = refused.message("satz_interview");
    match &refused.restore {
        Restore::Restored(path) => {
            assert_eq!(path, &session.main);
            assert!(
                message
                    .ends_with("satz refused and had changed new.satz; the file is back as it was"),
                "{message}"
            );
        }
        Restore::Untouched => assert_eq!(message, outcome.text),
        Restore::Failed(e) => panic!("{e}"),
    }

    // a refusal that wrote nothing: the file untouched, satz's sentence and no more
    let args = AddPackArgs {
        pack: BILLING.to_string(),
        with_requirements: false,
    };
    let Delegated::NotLanded(refused) = support::delegate(&session, "satz_add_pack", &args).await
    else {
        panic!("billing without a security model landed")
    };
    let Cause::Refused(outcome) = &refused.cause else {
        panic!("{refused:?}")
    };
    assert!(matches!(refused.restore, Restore::Untouched), "{refused:?}");
    assert_eq!(refused.message("satz_add_pack"), outcome.text);
    assert!(!outcome.text.contains("had changed"), "{}", outcome.text);
    assert_eq!(support::read(&main), before);
    assert!(estate.temp_files().is_empty(), "{:?}", estate.temp_files());
}

/// A `use` line stripped of its ` when <gate>`: nothing the estate answers switches that
/// pack off. satz says so with the `ungated-pack` finding while the file declaring the
/// gate deploys — here the map — and the Packs view's chip and sentence stand on the row
/// state and that finding.
#[tokio::test]
async fn a_line_without_its_when_is_ungated_and_satz_names_the_gate() {
    let estate = support::estate_dir(None);
    let main = estate.create_skeleton("new.satz").await;
    let session = estate.open("new.satz").await;
    support::answer_like_the_smoke_matrix(&session).await;
    support::add_pack(&session, support::MAP).await;
    support::add_pack(&session, BUDGET).await;

    // the gate comes off the line by hand, as an estate written without it has it; the
    // binding `use_budget = true` stays where satz wrote it
    let gated = format!("use \"{BUDGET}\" when use_budget");
    let text = support::read(&main);
    assert!(text.contains(&gated), "{text}");
    let ungated = text.replacen(&gated, &format!("use \"{BUDGET}\""), 1);
    std::fs::write(&main, &ungated).unwrap();

    // the row: the line is ungated and the pack deploys all the same
    let m = support::model(&session, Vec::new()).await;
    let row = m.packs.row(BUDGET).unwrap();
    assert_eq!((row.line, row.deploys), (PackLine::Ungated, true));
    assert_eq!(row.gate.as_deref(), Some("use_budget"));
    assert!(row.at_line.is_some(), "{row:?}");

    // the report: the finding on the pack, with satz's own sentence on the row
    let finding = m
        .packs
        .findings
        .iter()
        .find(|f| f.kind == "ungated-pack")
        .unwrap_or_else(|| panic!("no ungated-pack finding: {:?}", m.packs.findings));
    assert_eq!(finding.subject.as_deref(), Some(BUDGET));
    assert!(
        finding.message.contains("when use_budget"),
        "{}",
        finding.message
    );
    assert!(
        row.findings.iter().any(|f| f.contains("when use_budget")),
        "{:?}",
        row.findings
    );

    // and the check: at validation level `error` the same finding refuses the estate,
    // as the diagnostic the drawer shows
    let strict_dir = support::config_over(&estate.yaml, "error");
    let strict = CliChecker {
        cli: strict_dir.cli().await,
    };
    let err = support::within(strict.check(&main)).await.unwrap_err();
    let CheckFailure::Refused(refused) = err else {
        panic!("{err:?}")
    };
    let diagnostic = refused
        .iter()
        .find(|d| d.kind.as_deref() == Some("ungated-pack"))
        .unwrap_or_else(|| panic!("no ungated-pack diagnostic: {refused:?}"));
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.source, DiagSource::Check);
    assert!(
        diagnostic.message.contains("when use_budget"),
        "{}",
        diagnostic.message
    );
    assert_eq!(support::read(&main), ungated, "the check writes nothing");
    assert!(estate.temp_files().is_empty(), "{:?}", estate.temp_files());
}
