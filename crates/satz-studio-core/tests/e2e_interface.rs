//! A central estate's interfaces and a project estate that reads one, over satz's own
//! smoke files. `satz transpile` of the showcase writes `interfaces/` beside `hcl/`: every
//! file satz generates there is a tree that is the file, with the `interface "<name>"`
//! header and nothing satz refuses, and discovery walks past the folder as it walks past
//! `hcl/`. The smoke project takes the showcase's `interfaces/archive/` whole as
//! `vendor/archive/`, as satz's smoke matrix does: `satz_packs` names its interface file
//! as an interface and not as a file the graph does not know, the model builds with the
//! `${{interface.<export>}}` strings as the values they are, and the app's writer moves
//! one reference to another export and back through satz's check, while a reference to an
//! export the interface does not carry is refused and rolled back.

#[path = "fixtures/e2e/support.rs"]
mod e2e;
#[path = "fixtures/edit/support.rs"]
mod support;

use std::path::{Path, PathBuf};

use satz_studio_core::cst::{Cst, NodeKind, TypedValue};
use satz_studio_core::edit::{CommitError, Edit, EditSession, Rollback};
use satz_studio_core::estate::EstateDir;
use satz_studio_core::model::{ResourceNode, SourceValue};
use satz_studio_core::satz::EstateSession;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// The interface file the smoke project `use`s, relative to its root.
const INTERFACE: &str = "vendor/archive/archive/satz/interface.satz";

/// Every `.satz` file below `dir`.
fn satz_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            satz_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("satz") {
            out.push(path);
        }
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

/// `satz --config <dir> transpile showcase.satz` through the app's CLI runner, which
/// writes `hcl/` and `interfaces/` into the copy.
async fn transpile(session: &EstateSession) {
    let args = vec!["transpile".to_string(), session.main.display().to_string()];
    let (tx, mut rx) = mpsc::channel(1024);
    let collect = tokio::spawn(async move {
        let mut lines = Vec::new();
        while let Some(line) = rx.recv().await {
            lines.push(line);
        }
        lines
    });
    let status = e2e::within(session.cli.run(&args, tx, CancellationToken::new()))
        .await
        .unwrap();
    let lines = collect.await.unwrap();
    assert!(status.success(), "satz transpile failed: {lines:?}");
}

/// The smoke project beside `central`: `satz/archive.satz` from the pinned submodule, a
/// `config.toml` naming the schema and the presets of `vendor/satz` by absolute path, and
/// the central estate's `interfaces/archive/` copied whole to `vendor/archive/`.
fn project_beside(central: &Path) -> (tempfile::TempDir, PathBuf) {
    let dir = support::scratch();
    let root = dir.path().canonicalize().unwrap();
    let root = satz_studio_core::diag::plain(&root);
    let vendor = e2e::vendor();
    std::fs::create_dir_all(root.join("satz")).unwrap();
    std::fs::copy(
        vendor.join("tests/smoke/project/satz/archive.satz"),
        root.join("satz/archive.satz"),
    )
    .unwrap();
    std::fs::write(
        root.join("config.toml"),
        format!(
            "yaml_dir = \"satz\"\nhcl_dir = \"hcl\"\ninterfaces_dir = \"interfaces\"\ninclude_dirs = [\".\", \"satz\"]\nschema_dir = '{}'\npresets_dir = '{}'\ntf_tool = \"tofu\"\ngoogle_providers = [\"google\", \"google-beta\"]\nprovider_version = \"7.14.1\"\n",
            vendor.join("tests").join("schemas").display(),
            vendor.join("presets").display(),
        ),
    )
    .unwrap();
    copy_dir(
        &central.join("interfaces").join("archive"),
        &root.join("vendor").join("archive"),
    );
    (dir, root)
}

fn node<'a>(nodes: &'a [ResourceNode], key: &str) -> &'a ResourceNode {
    nodes
        .iter()
        .find(|n| n.key == key)
        .unwrap_or_else(|| panic!("no node `{key}`"))
}

/// The raw text of the string attribute `key` of `google_project.archive_work`.
fn work_attr(outline: &[ResourceNode], key: &str) -> String {
    let work = node(&node(outline, "google_project").children, "archive_work");
    let row = work
        .attrs
        .iter()
        .find(|a| a.key == key)
        .unwrap_or_else(|| panic!("no attribute {key}"));
    match &row.value {
        SourceValue::Str { raw, .. } => raw.clone(),
        other => panic!("{key}: {other:?}"),
    }
}

#[tokio::test]
async fn the_generated_interfaces_parse_and_a_project_reads_one() {
    // the central estate: the showcase, transpiled where the test may write
    let central = support::copy_smoke();
    let session = central.open("showcase.satz").await;
    transpile(&session).await;
    let interfaces = central.root.join("interfaces");
    let mut files = Vec::new();
    satz_files(&interfaces, &mut files);
    files.sort();
    assert!(
        files
            .iter()
            .any(|f| f.ends_with("archive/archive/satz/interface.satz")),
        "{files:?}"
    );
    assert!(
        files
            .iter()
            .any(|f| f.starts_with(interfaces.join("common"))),
        "{files:?}"
    );
    for f in &files {
        let text = support::read(f);
        let cst = Cst::parse(&text).unwrap();
        assert_eq!(cst.text(), text, "{}", f.display());
        assert!(
            cst.nodes()
                .all(|(_, n)| !matches!(n.kind, NodeKind::Error { .. })),
            "{}: an Error node in a file satz wrote",
            f.display()
        );
        let file = cst
            .lower()
            .unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let iface = file
            .interface_file
            .unwrap_or_else(|| panic!("{}: not an interface file", f.display()));
        let header = cst
            .nodes()
            .find_map(|(_, n)| match &n.kind {
                NodeKind::Header { keyword, name } => Some((keyword.clone(), name.clone())),
                _ => None,
            })
            .unwrap();
        assert_eq!(header, ("interface".to_string(), iface.name.clone()));
    }
    // generated output, like hcl/: discovery finds the one estate and walks past both
    assert_eq!(
        EstateDir::discover(&central.root),
        vec![central.root.join("config.toml")]
    );

    // the project estate that reads the archive interface
    let (_dir, project) = project_beside(&central.root);
    let bin = e2e::satz().await;
    let session = e2e::within(EstateSession::open(
        &bin,
        EstateDir::open(&project).unwrap(),
        PathBuf::from("archive.satz"),
    ))
    .await
    .unwrap();
    let model = e2e::model(&session, Vec::new()).await;
    assert_eq!(
        model
            .packs
            .interfaces
            .iter()
            .map(|u| u.path.as_str())
            .collect::<Vec<_>>(),
        [INTERFACE],
        "satz_packs names the interface file as an interface"
    );
    assert!(
        model.packs.unmanaged.iter().all(|u| u.path != INTERFACE),
        "{:?}",
        model.packs.unmanaged
    );
    assert!(model.uses.iter().any(|u| u.path == INTERFACE));
    assert_eq!(
        work_attr(&model.outline, "folder_id"),
        "${{interface.infra_folder}}"
    );

    // the writer: one reference to another export, through satz's check, and back
    let (mcp, _) = support::checkers(&session);
    let original = support::read(&session.main);
    let es = EditSession::open(&session.main).unwrap();
    let central_label = support::attr_named(es.cst(), "central");
    let proposed = es
        .apply(&[Edit::ReplaceValue {
            node: central_label,
            value: TypedValue::Raw("\"${{interface.infra_project_id}}\"".to_string()),
        }])
        .unwrap();
    support::within(proposed.commit(&mcp)).await.unwrap();
    let edited = support::read(&session.main);
    assert_eq!(
        edited,
        original.replacen(
            "\"${{interface.customer_domain}}\"",
            "\"${{interface.infra_project_id}}\"",
            1
        )
    );

    // an export no used interface file carries: satz refuses it, the bytes stay
    let es = EditSession::open(&session.main).unwrap();
    let central_label = support::attr_named(es.cst(), "central");
    let proposed = es
        .apply(&[Edit::ReplaceValue {
            node: central_label,
            value: TypedValue::Raw("\"${{interface.no_such_export}}\"".to_string()),
        }])
        .unwrap();
    let err = support::within(proposed.commit(&mcp)).await.unwrap_err();
    let CommitError::Rollback(Rollback::Check(diags)) = err else {
        panic!("{err}")
    };
    assert!(
        diags
            .iter()
            .any(|d| d.kind.as_deref() == Some("interface-use")
                && d.message.contains("no_such_export")),
        "{diags:?}"
    );
    assert_eq!(support::read(&session.main), edited);

    let es = EditSession::open(&session.main).unwrap();
    let central_label = support::attr_named(es.cst(), "central");
    let proposed = es
        .apply(&[Edit::ReplaceValue {
            node: central_label,
            value: TypedValue::Raw("\"${{interface.customer_domain}}\"".to_string()),
        }])
        .unwrap();
    support::within(proposed.commit(&mcp)).await.unwrap();
    assert_eq!(support::read(&session.main), original);
}
