//! satz's own installer, run from the window while no satz is found.
//!
//! The core does the part that must be right — the installer and its SHA-256 sidecar from
//! one release, compared before anything runs, run with `SATZ_NO_MODIFY_PATH=1` and a
//! closed stdin (`satz_studio_core::satz::install`). This is the run as the window sees
//! it: the guard, the log, the cancel, and satz located again afterwards.
//!
//! On Windows there is nothing to run: satz publishes no Windows build, and the action is
//! the sentence that says so.

use dioxus::prelude::*;
use tokio_util::sync::CancellationToken;

use super::{AppStore, ToastKind, toast};

#[cfg(not(windows))]
use satz_studio_core::github;
#[cfg(not(windows))]
use satz_studio_core::satz::install::{self, INSTALLER, NO_MODIFY_PATH, SATZ_REPO, SIDECAR};
#[cfg(not(windows))]
use satz_studio_core::satz::{CliLine, SatzError};

#[cfg(not(windows))]
use super::{
    AppStoreStoreExt, CommandOutcome, InstallStoreStoreExt, SatzStatus, app_actions::locate,
    strip_ansi,
};

/// Start the install and hand back what cancels it; `None` when nothing started.
#[cfg(windows)]
pub fn install_satz(app: Store<AppStore>) -> Option<CancellationToken> {
    use satz_studio_core::satz::install::InstallError;
    toast(
        app,
        ToastKind::Error,
        InstallError::NoWindowsBuild.to_string(),
    );
    None
}

/// Start the install and hand back what cancels it; `None` when nothing started.
///
/// It is refused while a satz is found at all — the offer is for an operator with none —
/// and while Settings names a satz binary: the installer writes `~/.local/bin/satz`, and
/// the search does not look there while a path is set, so the install would change
/// nothing the app runs.
#[cfg(not(windows))]
pub fn install_satz(app: Store<AppStore>) -> Option<CancellationToken> {
    if app.install().running().cloned() {
        toast(app, ToastKind::Info, "satz is already being installed");
        return None;
    }
    if !matches!(*app.satz().read(), SatzStatus::Missing(_)) {
        toast(
            app,
            ToastKind::Error,
            "the installer is offered only while no satz is found",
        );
        return None;
    }
    if let Some(path) = app.settings().read().satz_binary.clone() {
        toast(
            app,
            ToastKind::Error,
            format!(
                "Settings name {} as the satz binary, so a satz installed into ~/.local/bin would not be used — clear the path and save first",
                path.display()
            ),
        );
        return None;
    }

    let token = CancellationToken::new();
    let store = app.install();
    store.command().set(Some(format!(
        "{}={} sh {INSTALLER}   (the latest {SATZ_REPO} release, verified against its SHA-256 sidecar)",
        NO_MODIFY_PATH.0, NO_MODIFY_PATH.1
    )));
    store.log().clear();
    store.outcome().set(None);
    store.running().set(true);
    let cancel = token.clone();
    spawn(async move {
        let (outcome, cancelled) = run_installer(app, cancel).await;
        let outcome = if outcome.ok {
            // what is on disk changed, so the status the app holds is stale
            locate(app).await;
            let status = app.satz().read().clone();
            found_after_install(&status, outcome)
        } else {
            outcome
        };
        if !outcome.ok && !cancelled {
            toast(app, ToastKind::Error, outcome.text.clone());
        }
        app.install().outcome().set(Some(outcome));
        app.install().running().set(false);
    });
    Some(token)
}

/// The download, the check and the run, each line into the install log. The second half of
/// the answer is whether the operator cancelled, which is an outcome and not an error.
#[cfg(not(windows))]
async fn run_installer(app: Store<AppStore>, cancel: CancellationToken) -> (CommandOutcome, bool) {
    let failed = |text: String| (CommandOutcome { ok: false, text }, false);
    let mut log = app.install().log();
    log.push(CliLine::Stdout(format!(
        "reading the latest release of {SATZ_REPO}"
    )));
    let client = github::client();
    let fetched = tokio::select! {
        () = cancel.cancelled() => {
            return (
                CommandOutcome { ok: false, text: "cancelled — nothing was run".to_string() },
                true,
            );
        }
        fetched = install::fetch_verified(&client, github::API) => fetched,
    };
    let verified = match fetched {
        Ok(verified) => verified,
        Err(e) => return failed(e.to_string()),
    };
    log.push(CliLine::Stdout(format!(
        "{INSTALLER} of {} matches {SIDECAR} (sha256 {}); running it with {}={}",
        verified.release, verified.sha256, NO_MODIFY_PATH.0, NO_MODIFY_PATH.1
    )));

    let (tx, mut lines) = tokio::sync::mpsc::channel::<CliLine>(256);
    let child = cancel.clone();
    let join = tokio::spawn(async move { verified.run(tx, child).await });
    let mut last_stderr = None;
    while let Some(line) = lines.recv().await {
        let clean = match line {
            CliLine::Stdout(s) => CliLine::Stdout(strip_ansi(&s)),
            CliLine::Stderr(s) => {
                let s = strip_ansi(&s);
                if !s.trim().is_empty() {
                    last_stderr = Some(s.clone());
                }
                CliLine::Stderr(s)
            }
        };
        log.push(clean);
    }
    match join.await {
        Ok(Ok(status)) if status.success() => (
            CommandOutcome {
                ok: true,
                text: "the installer finished".to_string(),
            },
            false,
        ),
        // the installer said why on its own stderr; a status alone would replace that
        Ok(Ok(status)) => {
            failed(last_stderr.unwrap_or_else(|| format!("the installer exited with {status}")))
        }
        Ok(Err(SatzError::Cancelled)) => (
            CommandOutcome {
                ok: false,
                text: "cancelled — the installer was stopped where it stood".to_string(),
            },
            true,
        ),
        Ok(Err(e)) => failed(e.to_string()),
        Err(e) => failed(format!("the install task failed: {e}")),
    }
}

/// What an installer that exited cleanly left. A satz the search finds is the outcome; one
/// it still cannot use is a failure naming why — an exit status of zero is not the proof.
#[cfg(not(windows))]
fn found_after_install(status: &SatzStatus, finished: CommandOutcome) -> CommandOutcome {
    match status {
        SatzStatus::Located(bin) => CommandOutcome {
            ok: true,
            text: match bin.ahead_of_build() {
                None => format!("satz {} installed at {}", bin.version, bin.path.display()),
                Some(_) => format!(
                    "satz {} installed at {} — newer than the satz this build was tested against, which the banner says",
                    bin.version,
                    bin.path.display()
                ),
            },
        },
        SatzStatus::TooOld { found, .. } => CommandOutcome {
            ok: false,
            text: format!(
                "{}, but the satz found is {found}, which is too old",
                finished.text
            ),
        },
        SatzStatus::Missing(why) | SatzStatus::Unusable(why) => CommandOutcome {
            ok: false,
            text: format!("{}, but satz is still not usable: {why}", finished.text),
        },
        SatzStatus::Unknown => CommandOutcome {
            ok: false,
            text: format!("{}, and satz has not been located since", finished.text),
        },
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use std::path::PathBuf;

    use satz_studio_core::satz::SatzBinary;

    use super::*;

    fn finished() -> CommandOutcome {
        CommandOutcome {
            ok: true,
            text: "the installer finished".to_string(),
        }
    }

    #[test]
    fn a_clean_exit_is_an_install_only_when_the_search_then_finds_a_satz() {
        let built = SatzBinary::built_against();
        let bin = SatzBinary::check(PathBuf::from("/opt/satz"), built).unwrap();
        let located = found_after_install(&SatzStatus::Located(bin.clone()), finished());
        assert!(located.ok);
        assert!(located.text.contains("/opt/satz"), "{}", located.text);

        let missing = found_after_install(
            &SatzStatus::Missing("satz not found; tried satz (on PATH)".to_string()),
            finished(),
        );
        assert!(!missing.ok);
        assert!(
            missing.text.contains("still not usable"),
            "{}",
            missing.text
        );
    }
}
