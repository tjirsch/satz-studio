//! The estate map, driven as the Map view drives it, on the interviewed skeleton: the
//! map line uncommented by the app's rule and verified; a choice answered yes leaves
//! its pack line active, answered no leaves the line active and the gate false, which
//! the model shows as a note; a gate bound true whose line is gone is an `Absent` row,
//! and `satz transpile --check` names the pack — a warning at satz's default
//! validation level, a refusal at `error`.

#[path = "fixtures/e2e/support.rs"]
mod support;

use satz_studio_core::cst::{Cst, TypedValue, UseState, scan_uses};
use satz_studio_core::diag::{DiagSource, Severity, parse_satz_output};
use satz_studio_core::edit::{
    CheckFailure, Checker, CliChecker, CommitError, Edit, EditSession, McpChecker, Rollback,
    sha256_hex,
};
use satz_studio_core::model::{Choice, LineState, PackRowKind};
use satz_studio_core::satz::reports::QuestionState;
use std::sync::Arc;

const BILLING: &str = "presets/billing-account-permissions.satz";
const BUDGET: &str = "presets/organization-budget.satz";

#[tokio::test]
async fn the_map_goes_in_and_a_choice_answered_twice_leaves_its_line_active() {
    let estate = support::estate_dir(None);
    let main = estate.create_skeleton("new.satz").await;
    let session = estate.open("new.satz").await;
    support::answer_like_the_smoke_matrix(&session).await;

    let before = support::read(&main);
    let map_line = "// use \"presets/estate-map.satz\"\n";
    assert!(before.contains(map_line), "{before}");
    let committed = support::enable_map(&session).await;
    let text = support::read(&main);
    assert_eq!(
        text,
        before.replacen(map_line, "use \"presets/estate-map.satz\"\n", 1),
        "the `// ` goes and nothing else changes"
    );
    assert_eq!(committed.sha256, sha256_hex(text.as_bytes()));
    assert!(!committed.summary.addresses.is_empty());

    // with the map in, its choices are open
    let report = support::questions(&session).await;
    assert!(report.summary.total > 16, "{:?}", report.summary);
    let billing = report
        .questions
        .iter()
        .find(|q| q.subject == "use_billing_permissions")
        .expect("the map asks about the billing permissions");
    assert_eq!(billing.state, QuestionState::Unanswered);
    assert_eq!(billing.default, Some(serde_json::json!(true)));
    let commented = format!("\n// use \"{BILLING}\" when use_billing_permissions\n");
    let active = format!("\nuse \"{BILLING}\" when use_billing_permissions\n");
    assert!(text.contains(&commented));

    // yes: satz binds the param and uncomments the line
    let (report, _) = support::answer(
        &session,
        support::one_answer("use_billing_permissions", serde_json::json!(true)),
    )
    .await;
    assert_eq!(report.written, 1);
    let text = support::read(&main);
    assert!(text.contains(&active), "{text}");
    assert!(!text.contains(&commented));
    // by param, not by spacing: satz's writer keeps a formatted file formatted, so the
    // bound line carries the block's `=` column
    let cst = Cst::parse(&text).unwrap();
    assert_eq!(
        support::params_of(&cst)
            .get("use_billing_permissions")
            .map(String::as_str),
        Some("true"),
        "{text}"
    );
    let uses = scan_uses(&cst);
    let line = uses.iter().find(|u| u.path == BILLING).unwrap();
    assert_eq!(line.state, UseState::Active);
    assert_eq!(line.gate.as_deref(), Some("use_billing_permissions"));
    assert_eq!(
        uses.iter().find(|u| u.path == BUDGET).unwrap().state,
        UseState::Commented,
        "a choice left alone keeps its commented line"
    );
    let m = support::model(&session, Vec::new()).await;
    let row = m
        .packs
        .iter()
        .find(|r| r.gate.as_deref() == Some("use_billing_permissions"))
        .unwrap();
    assert_eq!(row.state, LineState::On);
    assert_eq!(
        row.choice,
        Choice::Bool {
            current: Some(true),
            default: None
        }
    );
    assert_eq!(row.line, Some(line.line));
    assert!(m.diagnostics.is_empty(), "{:?}", m.diagnostics);

    // no: the param reads false and the line stays active — satz never re-comments
    // one — which the model shows as a note on that line
    let (report, _) = support::answer(
        &session,
        support::one_answer("use_billing_permissions", serde_json::json!(false)),
    )
    .await;
    assert_eq!(report.written, 1);
    let text = support::read(&main);
    assert!(text.contains(&active), "{text}");
    assert_eq!(
        support::params_of(&Cst::parse(&text).unwrap())
            .get("use_billing_permissions")
            .map(String::as_str),
        Some("false"),
        "{text}"
    );
    let m = support::model(&session, Vec::new()).await;
    let row = m
        .packs
        .iter()
        .find(|r| r.gate.as_deref() == Some("use_billing_permissions"))
        .unwrap();
    assert_eq!(row.state, LineState::On);
    assert_eq!(
        row.choice,
        Choice::Bool {
            current: Some(false),
            default: None
        }
    );
    assert_eq!(m.diagnostics.len(), 1, "{:?}", m.diagnostics);
    let note = &m.diagnostics[0];
    assert_eq!(note.severity, Severity::Note);
    assert_eq!(note.source, DiagSource::Model);
    assert_eq!(note.file.as_deref(), Some(main.as_path()));
    assert_eq!(note.line, row.line);
    assert!(
        note.message.starts_with("line active, gate false"),
        "{}",
        note.message
    );
    assert!(
        note.message.contains("use_billing_permissions"),
        "{}",
        note.message
    );
    assert!(estate.temp_files().is_empty(), "{:?}", estate.temp_files());
}

#[tokio::test]
async fn a_gate_bound_true_without_its_line_is_absent_and_the_check_names_the_pack() {
    let estate = support::estate_dir(None);
    let main = estate.create_skeleton("new.satz").await;
    let session = estate.open("new.satz").await;
    support::answer_like_the_smoke_matrix(&session).await;
    support::enable_map(&session).await;

    // the budget line goes, as in an estate written before the library gained the pack
    let text = support::read(&main);
    let budget_line = format!("// use \"{BUDGET}\" when use_budget\n");
    assert!(text.contains(&budget_line), "{text}");
    std::fs::write(&main, text.replacen(&budget_line, "", 1)).unwrap();

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
    assert!(
        warning.message.contains(&format!(
            "`use_budget` is true and this estate has no line for `{BUDGET}`"
        )),
        "{}",
        warning.message
    );
    assert!(
        warning
            .message
            .contains("run `satz merge-presets` to write it"),
        "{}",
        warning.message
    );

    // the model: one Absent row, last, not a param row, and the warning carried
    let m = support::model(&session, diags.clone()).await;
    let absent: Vec<_> = m
        .packs
        .iter()
        .filter(|r| r.state == LineState::Absent)
        .collect();
    assert_eq!(absent.len(), 1, "{absent:?}");
    let row = absent[0];
    assert_eq!(row.kind, PackRowKind::Choice);
    assert_eq!(row.gate.as_deref(), Some("use_budget"));
    assert_eq!(row.path, None);
    assert_eq!(row.line, None);
    assert_eq!(
        row.choice,
        Choice::Bool {
            current: Some(true),
            default: None
        }
    );
    assert_eq!(
        row.question.as_ref().map(|q| q.state),
        Some(QuestionState::Answered)
    );
    assert_eq!(m.packs.last().unwrap().gate.as_deref(), Some("use_budget"));
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
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].severity, Severity::Error);
    assert_eq!(refused[0].source, DiagSource::Check);
    assert_eq!(refused[0].kind.as_deref(), Some("unadopted-pack"));
    assert!(
        refused[0]
            .message
            .starts_with("1 pack(s) this estate asks for but does not use"),
        "{}",
        refused[0].message
    );
    assert!(
        refused[0].message.contains(&format!(
            "`use_budget` is true and this estate has no line for `{BUDGET}` — run `satz merge-presets` to write it"
        )),
        "{}",
        refused[0].message
    );

    // … and the write discipline under it rolls the binding back: false lands, true
    // is refused with that diagnostic, the file keeps its bytes, no temp file stays
    let es = EditSession::open(&main).unwrap();
    let proposed = es
        .apply(&[Edit::ReplaceParam {
            name: "use_budget".to_string(),
            value: TypedValue::Bool(false),
        }])
        .unwrap();
    support::within(proposed.commit(&strict)).await.unwrap();
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
    assert!(estate.temp_files().is_empty(), "{:?}", estate.temp_files());
}
