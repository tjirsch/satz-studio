//! Workload packs through the installed satz, over satz's own `packs/` tree as a folder
//! source — nothing here reaches the network. `satz packs` says what the source holds;
//! `get-pack` fetches one and prints the `use` line; a pack edited here is reported with
//! its difference and left byte for byte as it was, and overwritten only when asked; a
//! pack the repository wrote is never one an update looks at; `detach-pack` makes a
//! fetched pack the repository's own. The `use` line satz names goes into the estate
//! file through the app's writer and satz's check.

#[path = "fixtures/e2e/support.rs"]
mod e2e;

use satz_studio_core::edit::{CliChecker, Edit, EditSession};
use satz_studio_core::satz::SatzError;
use satz_studio_core::satz::reports::{PackState, UpdateAction, WorkloadPackRow};
use satz_studio_core::satz::workload::{self, PackSource, Which};

const PACK: &str = "workload-project";
const ESTATE: &str = "estate wp\n\nparams {\n  customer_organization_id = \"123456789012\"\n}\n\nterraform {\n  backend {\n    local { path = \"terraform.tfstate\" }\n  }\n}\n";
/// A pack of the repository's own: params and nothing an estate has to answer.
const OWN: &str = "// A note every estate of this repository carries.\npack own version \"1.0\"\n\nparams {\n  own_note = \"kept\"\n}\n";

fn source() -> PackSource {
    workload::folder(&e2e::vendor().join("packs")).expect("satz ships a packs/ tree")
}

fn row<'a>(rows: &'a [WorkloadPackRow], id: &str) -> &'a WorkloadPackRow {
    rows.iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("no row for {id}: {rows:?}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pack_is_fetched_kept_current_left_alone_when_edited_and_detached() {
    let estate = e2e::estate_dir(None);
    std::fs::write(estate.file("wp.satz"), ESTATE).unwrap();
    let cli = estate.cli().await;
    let source = source();
    let file = estate.root.join("packs").join(format!("{PACK}.satz"));

    // nothing here yet: the source's pack is available
    let report = e2e::within(workload::list(&cli, &source)).await.unwrap();
    assert_eq!(row(&report.packs, PACK).state, PackState::Available);
    assert_eq!(row(&report.packs, PACK).use_line, None);

    // fetched: the file, the row, the line an estate reads it with
    let got = e2e::within(workload::get(&cli, &Which::One(PACK.into()), &source))
        .await
        .unwrap();
    assert_eq!(got.installed.len(), 1);
    assert_eq!(got.installed[0].id, PACK);
    let use_line = format!("use \"packs/{PACK}.satz\"");
    assert_eq!(got.installed[0].use_line, use_line);
    let fetched = e2e::read(&file);
    assert!(estate.root.join("packs").join("origin.toml").is_file());

    let report = e2e::within(workload::list(&cli, &source)).await.unwrap();
    let fetched_row = row(&report.packs, PACK);
    assert_eq!(fetched_row.state, PackState::Current);
    assert_eq!(fetched_row.source.as_deref(), Some("dir"));
    assert_eq!(
        fetched_row.use_path(),
        Some(format!("packs/{PACK}.satz").as_str())
    );

    // a second fetch is satz's refusal, not a report
    let again = e2e::within(workload::get(&cli, &Which::One(PACK.into()), &source)).await;
    match again {
        Err(SatzError::Exit { stderr, .. }) => {
            assert!(stderr.contains("update-pack"), "{stderr}")
        }
        other => panic!("a second get-pack was not refused: {other:?}"),
    }

    // the repository's own pack beside it: listed as own, and no update looks at it
    std::fs::write(estate.root.join("packs").join("own.satz"), OWN).unwrap();
    let report = e2e::within(workload::list(&cli, &source)).await.unwrap();
    assert_eq!(row(&report.packs, "own").state, PackState::Own);
    assert_eq!(row(&report.packs, "own").source, None);

    // edited here: reported with its difference, attention, and the file as it was
    let edited = format!("{fetched}// edited here\n");
    std::fs::write(&file, &edited).unwrap();
    let update = e2e::within(workload::update(&cli, &Which::All, &source, false))
        .await
        .unwrap();
    assert!(update.attention);
    assert_eq!(update.packs.len(), 1, "the own pack is no row: {update:?}");
    assert_eq!(update.packs[0].id, PACK);
    assert_eq!(update.packs[0].action, UpdateAction::Edited);
    let diff = update.packs[0]
        .diff
        .as_deref()
        .expect("an edited pack carries its diff");
    assert!(diff.contains("-// edited here"), "{diff}");
    assert_eq!(e2e::read(&file), edited);
    let report = e2e::within(workload::list(&cli, &source)).await.unwrap();
    assert_eq!(row(&report.packs, PACK).state, PackState::Edited);

    // overwritten only when asked
    let update = e2e::within(workload::update(
        &cli,
        &Which::One(PACK.into()),
        &source,
        true,
    ))
    .await
    .unwrap();
    assert!(!update.attention);
    assert_eq!(update.packs[0].action, UpdateAction::TookUpstream);
    assert_eq!(e2e::read(&file), fetched);

    // detached: the row goes, the file stays, and the pack is the repository's own
    let detached = e2e::within(workload::detach(&cli, PACK)).await.unwrap();
    assert_eq!(
        (detached.id.as_str(), detached.source.as_str()),
        (PACK, "dir")
    );
    assert_eq!(e2e::read(&file), fetched);
    let report = e2e::within(workload::list(&cli, &source)).await.unwrap();
    assert_eq!(row(&report.packs, PACK).state, PackState::Own);
    let update = e2e::within(workload::update(&cli, &Which::All, &source, false))
        .await
        .unwrap();
    assert!(update.packs.is_empty() && !update.attention, "{update:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_use_line_satz_names_goes_into_the_estate_through_the_writer() {
    let estate = e2e::estate_dir(None);
    let main = estate.file("wp.satz");
    std::fs::write(&main, ESTATE).unwrap();
    std::fs::create_dir_all(estate.root.join("packs")).unwrap();
    std::fs::write(estate.root.join("packs").join("own.satz"), OWN).unwrap();
    let cli = estate.cli().await;

    let report = e2e::within(workload::list(&cli, &source())).await.unwrap();
    let path = row(&report.packs, "own")
        .use_path()
        .expect("a pack that is here has a use line")
        .to_string();
    assert_eq!(path, "packs/own.satz");

    let es = EditSession::open(&main).unwrap();
    let proposed = es.apply(&[Edit::AddUse { path: path.clone() }]).unwrap();
    let checker = CliChecker { cli: cli.clone() };
    e2e::within(proposed.commit(&checker)).await.unwrap();
    let written = e2e::read(&main);
    assert_eq!(
        written,
        ESTATE.replace(
            "params {\n  customer_organization_id = \"123456789012\"\n}\n",
            "params {\n  customer_organization_id = \"123456789012\"\n}\n\nuse \"packs/own.satz\"\n"
        )
    );
    assert!(estate.temp_files().is_empty(), "{:?}", estate.temp_files());

    // the estate reads the pack now, and a second line for it is refused before a write
    let (passed, lines) = e2e::check_lines(&cli, &main).await;
    assert!(passed, "{lines:?}");
    let es = EditSession::open(&main).unwrap();
    assert!(es.apply(&[Edit::AddUse { path }]).is_err());
}
