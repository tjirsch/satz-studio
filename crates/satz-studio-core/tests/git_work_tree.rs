//! What `satz merge-presets` needs from git, asked the way the Overview asks it, and the
//! repository the Overview offers to make, made the way the app makes it: `init_steps`
//! run through `git::run` in the estate directory, streamed.
//!
//! Every directory here is in the SYSTEM temporary directory, not the repository's
//! drive: a scratch directory under this checkout's `target/` is inside this
//! repository's work tree, and git would rightly answer that it is held.

use std::path::Path;
use std::process::ExitStatus;
use std::time::Duration;

use satz_studio_core::git::{self, WorkTree};
use satz_studio_core::satz::CliLine;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const TIME_BOX: Duration = Duration::from_secs(60);

/// `git <args…>` through the app's runner: the status and every line it streamed.
async fn run(dir: &Path, args: &[String]) -> (ExitStatus, Vec<CliLine>) {
    let (tx, mut rx) = mpsc::channel(256);
    let collect = tokio::spawn(async move {
        let mut lines = Vec::new();
        while let Some(line) = rx.recv().await {
            lines.push(line);
        }
        lines
    });
    let status = tokio::time::timeout(TIME_BOX, git::run(dir, args, tx, CancellationToken::new()))
        .await
        .expect("git finished within the time box")
        .unwrap();
    (status, collect.await.unwrap())
}

fn words(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| a.to_string()).collect()
}

/// An estate directory as `satz init` leaves it: the config, the `.gitignore` beside it,
/// the estate file under `yaml/`, and a state file the `.gitignore` keeps out.
fn estate(root: &Path) {
    std::fs::create_dir_all(root.join("yaml")).unwrap();
    std::fs::create_dir_all(root.join("hcl")).unwrap();
    std::fs::write(root.join("config.toml"), "yaml_dir = \"yaml\"\n").unwrap();
    std::fs::write(
        root.join(".gitignore"),
        ".terraform/\n*.tfstate\nschemas/\n",
    )
    .unwrap();
    std::fs::write(
        root.join("yaml").join("C0example.satz"),
        "estate C0example\n",
    )
    .unwrap();
    std::fs::write(root.join("hcl").join("terraform.tfstate"), "{}\n").unwrap();
}

#[tokio::test]
async fn an_estate_outside_every_repository_is_outside_in_git_s_own_words() {
    let dir = tempfile::tempdir().unwrap();
    estate(dir.path());
    match WorkTree::read(&dir.path().join("yaml")).await {
        WorkTree::Outside(said) => assert!(said.contains("not a git repository"), "{said}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_steps_are_init_on_main_add_everything_and_one_commit_naming_the_estate() {
    let [init, add, commit] = git::init_steps("C0example.satz");
    assert_eq!(init, ["init", "-b", "main"]);
    assert_eq!(add, ["add", "-A"]);
    assert_eq!(commit[..2], ["commit", "-m"]);
    assert!(commit[2].contains("C0example.satz"), "{}", commit[2]);
}

/// The three steps, with an identity the repository itself carries — the test sets it
/// between the first step and the rest, because a CI runner has none of its own.
#[tokio::test]
async fn the_steps_put_the_estate_file_s_directory_in_a_work_tree_with_one_commit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    estate(root);
    let [init, add, commit] = git::init_steps("C0example.satz");

    let (status, lines) = run(root, &init).await;
    assert!(status.success(), "{lines:?}");
    assert!(!lines.is_empty(), "git init says what it made");
    for (key, value) in [("user.name", "acme"), ("user.email", "acme@example.com")] {
        let (status, lines) = run(root, &words(&["config", key, value])).await;
        assert!(status.success(), "{lines:?}");
    }
    for step in [add, commit] {
        let (status, lines) = run(root, &step).await;
        assert!(status.success(), "{step:?}: {lines:?}");
    }

    // where satz asks: the estate file's directory, one below the repository's root
    assert_eq!(WorkTree::read(&root.join("yaml")).await, WorkTree::Inside);
    let (_, subject) = run(root, &words(&["log", "--format=%s"])).await;
    assert_eq!(
        subject,
        [CliLine::Stdout(
            "the estate C0example.satz as it stands".to_string()
        )]
    );
    let (_, tracked) = run(root, &words(&["ls-files"])).await;
    let tracked: Vec<String> = tracked
        .into_iter()
        .map(|l| match l {
            CliLine::Stdout(s) | CliLine::Stderr(s) => s,
        })
        .collect();
    assert_eq!(
        tracked,
        [".gitignore", "config.toml", "yaml/C0example.satz"]
    );
}

/// With no identity git refuses the commit. The app sets none and invents none: the
/// refusal is git's own lines in the log, the status is a failure, and nothing is
/// committed.
#[tokio::test]
async fn a_commit_without_an_identity_is_git_s_refusal_and_commits_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    estate(root);
    let [init, add, commit] = git::init_steps("C0example.satz");
    assert!(run(root, &init).await.0.success());
    // an empty identity in the repository's own config outranks whatever the machine has
    for key in ["user.name", "user.email"] {
        assert!(run(root, &words(&["config", key, ""])).await.0.success());
    }
    assert!(run(root, &add).await.0.success());

    let (status, lines) = run(root, &commit).await;
    assert!(!status.success(), "{lines:?}");
    assert!(
        lines
            .iter()
            .any(|l| matches!(l, CliLine::Stderr(s) if s.starts_with("fatal:"))),
        "{lines:?}"
    );
    assert!(
        !run(root, &words(&["rev-parse", "--verify", "HEAD"]))
            .await
            .0
            .success()
    );
}
