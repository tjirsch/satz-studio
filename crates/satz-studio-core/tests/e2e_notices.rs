//! What a pack asks to be run once it is switched on, against the satz the submodule
//! pins: the answer that switches a CIS org-policy pack on comes back with the pack's
//! notice — the command and the param that acknowledges it — the compile warns at the
//! estate's `use` line while it stands, binding the param through `satz_interview` is
//! what takes it away, and satz never sends the same notice twice.
//!
//! This is the path the window's notice dialog stands on: the app holds what these
//! calls return, and `EstateDir::acknowledged` over the reload's params is what
//! removes it again.

#[path = "fixtures/e2e/support.rs"]
mod support;

use satz_studio_core::cst::Cst;
use satz_studio_core::estate::EstateDir;
use satz_studio_core::satz::CliLine;
use satz_studio_core::satz::reports::{FindingSeverity, QuestionState};

/// Everything `transpile --check` printed, whichever stream it used.
fn said(lines: &[CliLine]) -> String {
    lines
        .iter()
        .map(|l| match l {
            CliLine::Stdout(t) | CliLine::Stderr(t) => t.as_str(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The CIS baseline: the pack the map offers whose notice names `satz adopt`.
const PACK: &str = "presets/cis/CIS-GCP-Foundation-4.0.satz";
const GATE: &str = "use_cis_baseline";
const ACK: &str = "cis_baseline_adopted";

#[tokio::test]
async fn a_pack_switched_on_returns_its_notice_once_and_the_binding_takes_it_away() {
    let estate = support::estate_dir(None);
    let main = estate.create_skeleton("new.satz").await;
    let session = estate.open("new.satz").await;
    support::answer_like_the_smoke_matrix(&session).await;

    // a day-0 estate uses no pack that asks for anything
    let (report, _) = support::answer(
        &session,
        support::one_answer("default_region", serde_json::json!("europe-west3")),
    )
    .await;
    assert!(report.notices.is_empty(), "{:?}", report.notices);

    support::add_pack(&session, support::MAP).await;
    let open = support::questions(&session).await;
    let gate = open
        .questions
        .iter()
        .find(|q| q.subject == GATE)
        .expect("the map offers the CIS baseline");
    assert_eq!(gate.state, QuestionState::Unanswered);
    assert!(
        !open.questions.iter().any(|q| q.subject == ACK),
        "the acknowledgement is not a question: no pack asks it"
    );

    // the yes that switches the pack on brings its notice back with it
    let (report, _) =
        support::answer(&session, support::one_answer(GATE, serde_json::json!(true))).await;
    assert_eq!(report.written, 1);
    let notice = match report.notices.as_slice() {
        [one] => one.clone(),
        other => panic!("one notice opened, got {other:?}"),
    };
    assert_eq!(notice.param, ACK);
    assert_eq!(notice.pack, PACK);
    assert_eq!(notice.run, "satz adopt <estate> --execute --import");
    assert_eq!(notice.severity, FindingSeverity::Error);
    assert!(notice.holds_up_apply());
    assert!(!notice.acknowledged);
    assert!(!notice.text.is_empty());

    // satz reports it once: the next call opens nothing, and the estate still owes it
    let (again, _) = support::answer(
        &session,
        support::one_answer("cis_require_shielded_vm", serde_json::json!(false)),
    )
    .await;
    assert!(again.notices.is_empty(), "{:?}", again.notices);
    let env = session.dir.params(&session.main).unwrap();
    assert!(!EstateDir::acknowledged(&env, ACK), "not run, not bound");

    // and the compile says so at the estate's own `use` line, which is what the drawer
    // and the Overview's card count
    let (passed, lines) = support::check_lines(&estate.cli().await, &main).await;
    assert!(passed, "a notice is a warning, not a refusal");
    let printed = said(&lines);
    assert!(printed.contains("notices open"), "{printed}");
    assert!(printed.contains(ACK), "{printed}");

    // "I ran it": the param bound true through satz's own writer
    let (acknowledged, committed) =
        support::answer(&session, support::one_answer(ACK, serde_json::json!(true))).await;
    assert_eq!(acknowledged.written, 1);
    assert!(acknowledged.notices.is_empty());
    assert_eq!(committed.path, main);
    let env = session.dir.params(&session.main).unwrap();
    assert!(EstateDir::acknowledged(&env, ACK), "the estate has said so");
    assert!(
        support::params_of(&Cst::parse(&support::read(&main)).unwrap())
            .get(ACK)
            .is_some_and(|v| v == "true"),
        "the acknowledgement is a param the estate binds, in the file"
    );

    // the compile has nothing left to say about this one
    let (passed, lines) = support::check_lines(&estate.cli().await, &main).await;
    assert!(passed);
    let printed = said(&lines);
    assert!(!printed.contains(ACK), "{printed}");
    assert!(estate.temp_files().is_empty(), "{:?}", estate.temp_files());
}

/// The one answer satz takes for a notice is `true`: it is an acknowledgement, not a
/// decision, and a `false` is refused rather than written.
#[tokio::test]
async fn a_notice_is_acknowledged_with_true_and_with_nothing_else() {
    let estate = support::estate_dir(None);
    estate.create_skeleton("new.satz").await;
    let session = estate.open("new.satz").await;
    support::answer_like_the_smoke_matrix(&session).await;
    support::add_pack(&session, support::MAP).await;
    support::answer(&session, support::one_answer(GATE, serde_json::json!(true))).await;

    let args = serde_json::to_value(support::one_answer(ACK, serde_json::json!(false)))
        .unwrap()
        .as_object()
        .cloned()
        .unwrap();
    let outcome = support::within(session.tool("satz_interview", args))
        .await
        .unwrap();
    assert!(outcome.is_error, "{}", outcome.text);
    assert!(outcome.text.contains(ACK), "{}", outcome.text);
}
