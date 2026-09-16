//! `SatzCli::run_in`: the runner behind Create, which runs satz IN a directory and
//! passes no `--config`.
//!
//! The real `satz init` is never run here. It reads the Application Default Credentials
//! and asks Google for an organisation, a billing account and an identity, so running
//! it would put a live organisation id, a billing account id and an administrator's
//! address into a test's output. What is proved instead is everything the app decides:
//! the argv (`InitOptions::argv`, in the module's own tests), where the child runs, and
//! that no `--config` reaches it. A fake `satz` reports both back.

use satz_studio_core::satz::SatzError;
use satz_studio_core::satz::init::{check_target, created};

// the fake satz is a shell script, so everything that drives one is a unix fixture and
// so is what it needs
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
use satz_studio_core::satz::{CliLine, InitOptions, SatzBinary, SatzCli};
#[cfg(unix)]
use tokio::sync::mpsc;
#[cfg(unix)]
use tokio_util::sync::CancellationToken;

#[cfg(unix)]
#[path = "fixtures/satz/support.rs"]
mod support;

#[cfg(unix)]
const TIME_BOX: Duration = Duration::from_secs(60);

/// A binary record around a path. `locate` is not used: it would hold the fake to
/// `MIN_SATZ`, and what is under test here is the call, not the gate.
#[cfg(unix)]
fn binary(path: PathBuf) -> SatzBinary {
    SatzBinary {
        path,
        version: semver::Version::parse(satz_studio_core::satz::MIN_SATZ).unwrap(),
    }
}

/// Run `args` in `dir` through a fake satz and collect every line it streamed.
#[cfg(unix)]
async fn run_in(bin: &SatzBinary, dir: &Path, args: &[String]) -> (bool, Vec<CliLine>) {
    let (tx, mut rx) = mpsc::channel(64);
    let collect = tokio::spawn(async move {
        let mut lines = Vec::new();
        while let Some(line) = rx.recv().await {
            lines.push(line);
        }
        lines
    });
    let status = tokio::time::timeout(
        TIME_BOX,
        SatzCli::run_in(&bin.path, dir, args, tx, CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap();
    (status.success(), collect.await.unwrap())
}

#[cfg(unix)]
fn stdout(lines: &[CliLine]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|l| match l {
            CliLine::Stdout(s) => Some(s.clone()),
            CliLine::Stderr(_) => None,
        })
        .collect()
}

#[cfg(unix)]
fn stderr(lines: &[CliLine]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|l| match l {
            CliLine::Stderr(s) => Some(s.clone()),
            CliLine::Stdout(_) => None,
        })
        .collect()
}

/// The whole point of the runner: the child's working directory is the target, and
/// `--config` — which every other call passes and which satz refuses for a directory
/// holding no `config.toml` — is nowhere on the command line.
#[cfg(unix)]
#[tokio::test]
async fn the_child_runs_in_the_target_directory_and_is_given_no_config() {
    let home = tempfile::tempdir().unwrap();
    let bin = binary(support::fake(home.path(), r#"pwd; echo "argv: $*""#));
    let target = tempfile::tempdir().unwrap();
    // macOS puts the temporary directory behind /var -> /private/var; the child prints
    // the resolved path, so the expectation is resolved too.
    let expected = target.path().canonicalize().unwrap();

    let options = InitOptions {
        customer_shortname: "acme".to_string(),
        ..Default::default()
    };
    let (ok, lines) = run_in(&bin, target.path(), &options.argv()).await;

    assert!(ok);
    let out = stdout(&lines);
    assert_eq!(out[0], expected.display().to_string(), "{out:?}");
    assert_eq!(out[1], "argv: init --customer-shortname acme", "{out:?}");
    assert!(
        !out.iter().any(|l| l.contains("--config")),
        "a --config reached the child: {out:?}"
    );
}

/// The streaming shape is `run`'s: both pipes forwarded, kept apart, in order.
#[cfg(unix)]
#[tokio::test]
async fn both_pipes_are_streamed_and_stay_apart() {
    let home = tempfile::tempdir().unwrap();
    let bin = binary(support::fake(
        home.path(),
        "echo 'Created directory: yaml'; echo 'init: nothing could be derived' >&2; echo 'Initialization complete.'",
    ));
    let target = tempfile::tempdir().unwrap();
    let (ok, lines) = run_in(&bin, target.path(), &["init".to_string()]).await;

    assert!(ok);
    assert_eq!(
        stdout(&lines),
        ["Created directory: yaml", "Initialization complete."]
    );
    assert_eq!(stderr(&lines), ["init: nothing could be derived"]);
}

/// A satz that refuses is a non-zero exit with its own sentence on stderr — what the
/// form shows instead of an error of the app's own.
#[cfg(unix)]
#[tokio::test]
async fn a_refusal_is_the_exit_status_and_what_satz_said() {
    let home = tempfile::tempdir().unwrap();
    let bin = binary(support::fake(
        home.path(),
        "echo 'error: the credentials are not usable' >&2; exit 2",
    ));
    let target = tempfile::tempdir().unwrap();
    let (ok, lines) = run_in(&bin, target.path(), &["init".to_string()]).await;

    assert!(!ok);
    assert_eq!(stderr(&lines), ["error: the credentials are not usable"]);
}

/// A command that will not end is cancellable, as a streamed command is.
#[cfg(unix)]
#[tokio::test]
async fn a_run_that_hangs_is_cancelled() {
    let home = tempfile::tempdir().unwrap();
    // `--version` answers at once, or the helper's runnability probe would wait out the
    // sleep; `exec` replaces the shell, so the process the runner kills IS the sleep and
    // the pipes close with it rather than being held open by an orphan.
    let bin = binary(support::fake(
        home.path(),
        "case \"$1\" in --version) echo 'satz 0.0.0' ;; *) exec sleep 30 ;; esac",
    ));
    let target = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(8);
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let cancel = CancellationToken::new();
    let run = {
        let cancel = cancel.clone();
        let bin = bin.clone();
        let dir = target.path().to_path_buf();
        tokio::spawn(async move {
            SatzCli::run_in(&bin.path, &dir, &["init".to_string()], tx, cancel).await
        })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();
    let result = tokio::time::timeout(TIME_BOX, run).await.unwrap().unwrap();
    assert!(matches!(result, Err(SatzError::Cancelled)), "{result:?}");
    drain.await.unwrap();
}

/// The guard in front of the run, against the two directories a Create must not touch.
#[test]
fn the_target_is_checked_before_anything_runs() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(check_target(tmp.path()).is_ok());

    let missing = tmp.path().join("not-here");
    assert!(matches!(
        check_target(&missing),
        Err(SatzError::TargetMissing(_))
    ));

    std::fs::write(tmp.path().join("config.toml"), "yaml_dir = \"yaml\"\n").unwrap();
    assert!(matches!(
        check_target(tmp.path()),
        Err(SatzError::AlreadyAnEstate(_))
    ));
}

/// What `created` answers over the directory a real `init` leaves behind, built here by
/// hand so no live command runs: the estate is found by reading the directory, never by
/// predicting the name satz derives from the customer id.
#[test]
fn created_reads_the_estate_out_of_the_directory() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("yaml")).unwrap();
    std::fs::create_dir_all(tmp.path().join("hcl")).unwrap();
    std::fs::create_dir_all(tmp.path().join("schemas")).unwrap();
    std::fs::write(
        tmp.path().join("config.toml"),
        "yaml_dir = \"yaml\"\nhcl_dir = \"hcl\"\nschema_dir = \"schemas\"\n",
    )
    .unwrap();
    // satz names the file after the customer id; C0example is satz's example customer
    std::fs::write(
        tmp.path().join("yaml").join("C0example.satz"),
        "estate acme\n\nparams {\n  customer_shortname = \"acme\"\n}\n",
    )
    .unwrap();

    let (dir, estates) = created(tmp.path()).unwrap();
    assert_eq!(dir.dir, tmp.path());
    assert_eq!(
        estates
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        ["C0example.satz"]
    );
}
