//! The pack rows and the edges between them are satz's: the model's pack report, read
//! over the session as the app's reload reads it (`satz_packs`), equals what
//! `satz packs <estate> --format json` writes for the same estate — every row, every
//! requirement and dependent, every `use` the graph does not know, every finding. The
//! estates are the skeleton `satz interview --create` writes, as written and interviewed
//! with the map and two packs on, so a pin bump that changes the graph is checked here
//! without anyone naming it.

#[path = "fixtures/e2e/support.rs"]
mod support;

use std::path::Path;

use satz_studio_core::model::EstateModel;
use satz_studio_core::satz::reports::{Finding, PacksReport, RemovePackArgs};

const LOGSINK: &str = "presets/monitoring/organization-audit-logsink.satz";
const ALERTS: &str = "presets/monitoring/organization-cis-log-alerts-central.satz";
const MAIL: &str = "presets/scc/scc-findings-mail.satz";
const SENTINEL: &str = "presets/integrations/microsoft-sentinel.satz";
const BILLING: &str = "presets/billing-account-permissions.satz";
const S1: &str = "presets/security-group-models/s1-security-groups.satz";
const RUNNER: &str = "presets/ci/verification-runner.satz";
const GRANT: &str = "presets/ci/verification-runner-grant.satz";

async fn cli_report(estate: &support::Estate, main: &Path) -> PacksReport {
    let cli = estate.cli().await;
    support::within(cli.json_report(&["packs".to_string(), main.display().to_string()]))
        .await
        .unwrap()
}

/// A finding without the half that names the estate as typed: the CLI names the file it
/// was given and the server the estate it opened.
fn judged(findings: &[Finding]) -> Vec<(String, Option<String>, String, Option<u32>)> {
    findings
        .iter()
        .map(|f| (f.kind.clone(), f.subject.clone(), f.message.clone(), f.line))
        .collect()
}

fn holds_to_the_cli(model: &EstateModel, cli: &PacksReport) {
    assert_eq!(model.packs.note, cli.note);
    assert_eq!(model.packs.packs.len(), cli.packs.len());
    for (app, satz) in model.packs.packs.iter().zip(&cli.packs) {
        assert_eq!(app, satz, "the row of {}", satz.path);
    }
    assert_eq!(model.packs.unmanaged, cli.unmanaged);
    assert_eq!(judged(&model.packs.findings), judged(&cli.findings));
}

/// `path` needs `on`, alone or among others.
fn needs(report: &PacksReport, path: &str, on: &str) -> bool {
    report
        .row(path)
        .unwrap_or_else(|| panic!("no row for {path}"))
        .requires
        .iter()
        .any(|r| r.any_of.iter().any(|p| p == on))
}

#[tokio::test]
async fn the_skeleton_s_rows_are_the_rows_satz_packs_writes() {
    let estate = support::estate_dir(None);
    let main = estate.create_skeleton("C0example.satz").await;
    let session = estate.open("C0example.satz").await;
    let model = support::model(&session, Vec::new()).await;
    let cli = cli_report(&estate, &main).await;
    holds_to_the_cli(&model, &cli);

    // the dependencies no `ask_when` declares are in the rows all the same
    let r = &model.packs;
    assert!(needs(r, ALERTS, LOGSINK), "central alerts need the archive");
    assert!(
        needs(r, MAIL, ALERTS),
        "the findings mail needs the central alerts"
    );
    assert!(needs(r, SENTINEL, LOGSINK), "Sentinel needs the archive");
    assert!(
        needs(r, BILLING, S1),
        "billing needs a security-group model"
    );
    assert!(needs(r, GRANT, RUNNER), "the runner grant needs the runner");
    assert!(
        r.row(LOGSINK)
            .unwrap()
            .required_by
            .iter()
            .any(|p| p == ALERTS)
    );
}

#[tokio::test]
async fn an_interviewed_estate_s_rows_are_the_rows_satz_packs_writes() {
    let estate = support::estate_dir(None);
    let main = estate.create_skeleton("C0example.satz").await;
    let session = estate.open("C0example.satz").await;
    support::answer_like_the_smoke_matrix(&session).await;
    support::add_pack(&session, support::MAP).await;
    support::add_pack(&session, LOGSINK).await;
    support::add_pack(&session, ALERTS).await;
    let model = support::model(&session, Vec::new()).await;
    let alerts = model.packs.row(ALERTS).unwrap();
    assert!(alerts.deploys);
    assert!(
        alerts.requires.iter().all(|r| r.met),
        "{:?}",
        alerts.requires
    );
    let cli = cli_report(&estate, &main).await;
    holds_to_the_cli(&model, &cli);

    // what needs a pack keeps it on: satz refuses the switch, naming the dependent
    let refused = support::switch(
        &session,
        "satz_remove_pack",
        &RemovePackArgs {
            pack: LOGSINK.to_string(),
            cascade: false,
        },
    )
    .await
    .expect_err("the archive while the central alerts need it");
    assert!(refused.contains(ALERTS), "{refused}");
}
