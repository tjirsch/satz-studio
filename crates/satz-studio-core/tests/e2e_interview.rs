//! Interview parity with satz's smoke matrix (`vendor/satz/scripts/smoke.sh`, the
//! interview step): the sixteen day-0 answers the matrix pipes into `satz interview`,
//! given the app's way — one `satz_interview` call per typed value, then the defaults,
//! each verified by `satz transpile --check` through the estate's `satz mcp` — end in
//! the same estate the CLI-driven interview writes on a second copy: complete, with
//! the rename hint, the same params and the same `use` line states.

#[path = "fixtures/e2e/support.rs"]
mod support;

use satz_studio_core::cst::Cst;
use satz_studio_core::satz::reports::{QuestionState, QuestionsReport};

#[tokio::test]
async fn the_app_path_and_the_cli_path_end_in_the_same_estate() {
    let estate = support::estate_dir(None);
    let main = estate.create_skeleton("new.satz").await;
    let session = estate.open("new.satz").await;
    assert_eq!(session.main, main);

    // day 0: the scaffold's sixteen questions, nine of them without a usable default
    let open = support::questions(&session).await;
    assert_eq!(open.summary.total, 16);
    assert_eq!(open.summary.unanswered, 16);
    assert_eq!(open.summary.blocking, 9);
    assert!(!open.summary.complete);

    let mut last = None;
    for (i, (subject, value)) in support::TYPED.iter().enumerate() {
        let (report, committed) = support::answer(
            &session,
            support::one_answer(subject, serde_json::json!(value)),
        )
        .await;
        assert_eq!(report.written, 1, "{subject}");
        assert!(!report.created);
        assert_eq!(committed.path, main);
        assert_eq!(report.report.summary.answered, i + 1);
        // the report is the worklist (the app sends no filter): what was answered
        // has left it, what is open is in it
        assert_eq!(report.report.questions.len(), 16 - (i + 1));
        assert!(
            report
                .report
                .questions
                .iter()
                .all(|q| q.subject != *subject),
            "{subject} is still open"
        );
        assert!(
            report
                .report
                .questions
                .iter()
                .all(|q| q.state == QuestionState::Unanswered)
        );
        // the project id is offered once the short name is typed, not before
        let project = report
            .report
            .questions
            .iter()
            .find(|q| q.subject == "infra_project_name")
            .expect("infra_project_name is still open");
        if i < 3 {
            assert!(project.blocking, "{project:?}");
            assert_eq!(project.default, None);
        }
        if *subject == "customer_shortname" {
            assert!(!project.blocking, "{project:?}");
            assert_eq!(project.default, Some(serde_json::json!("acme-infra-001")));
        }
        last = Some(report);
    }
    // customer_id is bound: the file has the name `init` would have given it
    assert_eq!(
        last.unwrap().rename_to.as_deref(),
        Some("C0example.satz"),
        "the rename hint"
    );

    let (report, committed) = support::answer(&session, support::accept_defaults()).await;
    assert_eq!(
        report.written, 9,
        "the seven defaults and the two derived names"
    );
    assert!(report.report.summary.complete);
    assert_eq!(report.report.summary.answered, 16);
    assert_eq!(report.report.summary.unanswered, 0);
    assert_eq!(report.rename_to.as_deref(), Some("C0example.satz"));
    assert!(report.report.questions.is_empty(), "nothing is open");
    assert!(!committed.summary.addresses.is_empty());

    // the `questions` report the CLI writes says the same as the last report, and
    // lists every question answered with what was typed
    let cli: QuestionsReport = support::within(
        session
            .cli
            .json_report(&["questions".to_string(), main.display().to_string()]),
    )
    .await
    .unwrap();
    assert_eq!(cli.summary, report.report.summary);
    assert_eq!(cli.questions.len(), 16);
    assert!(
        cli.questions
            .iter()
            .all(|q| q.state == QuestionState::Answered)
    );
    for (subject, value) in support::TYPED {
        let row = cli.questions.iter().find(|q| q.subject == subject).unwrap();
        assert_eq!(row.current, Some(serde_json::json!(value)), "{subject}");
    }
    let over_mcp = support::questions(&session).await;
    assert_eq!(over_mcp.summary, cli.summary);
    assert_eq!(over_mcp.questions, cli.questions);

    // the CLI-driven interview on a second copy, fed what the smoke matrix pipes
    let other = support::estate_dir(None);
    let other_main = other
        .interview_cli("new.satz", &support::smoke_input())
        .await;
    let ours = Cst::parse(&support::read(&main)).unwrap();
    let theirs = Cst::parse(&support::read(&other_main)).unwrap();
    let (a, b) = (support::params_of(&ours), support::params_of(&theirs));
    assert_eq!(a.len(), 16, "{a:?}");
    assert_eq!(a, b, "the same params, whichever client wrote them");
    assert_eq!(a["customer_shortname"], "\"acme\"");
    assert_eq!(a["infra_project_name"], "\"acme-infra-001\"");
    assert_eq!(support::use_states(&ours), support::use_states(&theirs));
    assert_eq!(
        support::use_states(&ours)
            .iter()
            .filter(|(_, _, active)| *active)
            .map(|(path, _, _)| path.as_str())
            .collect::<Vec<_>>(),
        ["presets/estate-core.satz"],
        "a day-0 estate uses estate-core and nothing else"
    );
    // and outside `params { }` the two files are the same skeleton, byte for byte
    assert_eq!(
        support::outside_params(&ours),
        support::outside_params(&theirs)
    );
    assert!(estate.temp_files().is_empty(), "{:?}", estate.temp_files());
    assert!(other.temp_files().is_empty(), "{:?}", other.temp_files());
}
