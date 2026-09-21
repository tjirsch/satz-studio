//! satz's own installer, run from the window: while no satz is found, and on Windows for
//! an update, where `satz self-update` refuses and names the PowerShell one-liner.
//!
//! The core does the part that must be right — the installer of this system and its SHA-256
//! sidecar from one release, compared before anything runs, run with `SATZ_INSTALL_DIR`
//! naming the folder, `SATZ_NO_MODIFY_PATH=1` and a closed stdin
//! (`satz_studio_core::satz::install`). This is the run as the window sees it: the guard,
//! the log, the cancel, and satz located again afterwards.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use satz_studio_core::github;
use satz_studio_core::satz::install::{self, Installer, SATZ_REPO};
use satz_studio_core::satz::{CliLine, SatzBinary, SatzError};
use tokio_util::sync::CancellationToken;

use super::{
    AppStore, AppStoreStoreExt, CommandOutcome, InstallStoreStoreExt, SatzStatus, ToastKind,
    UpdateStoreStoreExt, app_actions::locate, strip_ansi, toast,
};

/// Start the install and hand back what cancels it; `None` when nothing started.
///
/// It is refused while a satz is found at all — the offer is for an operator with none —
/// and while Settings names a satz binary: the installer writes `~/.local/bin/satz`, and
/// the search does not look there while a path is set, so the install would change
/// nothing the app runs.
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
    let Some(dir) = SatzBinary::home_bin_dir() else {
        toast(
            app,
            ToastKind::Error,
            "there is no home directory to install satz into — set the path to satz in Settings",
        );
        return None;
    };

    let token = CancellationToken::new();
    let store = app.install();
    store.command().set(Some(header(&dir)));
    store.log().clear();
    store.outcome().set(None);
    store.running().set(true);
    let cancel = token.clone();
    spawn(async move {
        let push = move |line| app.install().log().push(line);
        let (outcome, cancelled) = match run_installer(push, dir, cancel).await {
            Ran::Finished { release } => {
                // what is on disk changed, so the status the app holds is stale
                locate(app).await;
                let status = app.satz().read().clone();
                (found_after_install(&status, &release), false)
            }
            Ran::Failed(outcome) => (outcome, false),
            Ran::Cancelled(outcome) => (outcome, true),
        };
        if !outcome.ok && !cancelled {
            toast(app, ToastKind::Error, outcome.text.clone());
        }
        app.install().outcome().set(Some(outcome));
        app.install().running().set(false);
    });
    Some(token)
}

/// The update where `satz self-update` does not install — Windows: the installer of the
/// latest release, verified as for an install, run into the folder of the satz in use
/// (`satz`), streamed into the update log. The caller holds the guard against a second run.
pub fn update_by_installer(app: Store<AppStore>, satz: &Path) -> Option<CancellationToken> {
    let Some(dir) = satz.parent().map(Path::to_path_buf) else {
        toast(
            app,
            ToastKind::Error,
            format!("{} has no folder to install into", satz.display()),
        );
        return None;
    };
    let token = CancellationToken::new();
    let update = app.update();
    update.command().set(Some(header(&dir)));
    update.log().clear();
    update.outcome().set(None);
    update.running().set(true);
    let cancel = token.clone();
    spawn(async move {
        let push = move |line| app.update().log().push(line);
        let (outcome, cancelled) = match run_installer(push, dir, cancel).await {
            Ran::Finished { release } => {
                // the version on disk changed, and what a check found against the old one
                // is stale
                app.update().found().set(None);
                locate(app).await;
                let status = app.satz().read().clone();
                (found_after_update(&status, &release), false)
            }
            Ran::Failed(outcome) => (outcome, false),
            Ran::Cancelled(outcome) => (outcome, true),
        };
        if !outcome.ok && !cancelled {
            toast(app, ToastKind::Error, outcome.text.clone());
        }
        app.update().outcome().set(Some(outcome));
        app.update().running().set(false);
    });
    Some(token)
}

/// The log header of a run into `dir`.
fn header(dir: &Path) -> String {
    format!(
        "{}   (the latest {SATZ_REPO} release, verified against its SHA-256 sidecar)",
        Installer::for_this_system().command_line(dir)
    )
}

/// How a run of the installer ended. A cancel is an outcome, not an error.
enum Ran {
    /// the installer of `release` exited cleanly
    Finished {
        release: String,
    },
    Failed(CommandOutcome),
    Cancelled(CommandOutcome),
}

/// The download, the check and the run into `dir`, each line handed to `push`.
async fn run_installer(
    mut push: impl FnMut(CliLine),
    dir: PathBuf,
    cancel: CancellationToken,
) -> Ran {
    let failed = |text: String| Ran::Failed(CommandOutcome { ok: false, text });
    let installer = Installer::for_this_system();
    push(CliLine::Stdout(format!(
        "reading the latest release of {SATZ_REPO}"
    )));
    let client = github::client();
    let fetched = tokio::select! {
        () = cancel.cancelled() => {
            return Ran::Cancelled(
                CommandOutcome { ok: false, text: "cancelled — nothing was run".to_string() },
            );
        }
        fetched = install::fetch_verified(installer, &client, github::API) => fetched,
    };
    let verified = match fetched {
        Ok(verified) => verified,
        Err(e) => return failed(e.to_string()),
    };
    let release = verified.release.clone();
    push(CliLine::Stdout(format!(
        "{} of {release} matches {} (sha256 {}); running it into {}",
        installer.asset(),
        installer.sidecar(),
        verified.sha256,
        dir.display()
    )));

    let (tx, mut lines) = tokio::sync::mpsc::channel::<CliLine>(256);
    let child = cancel.clone();
    let join = tokio::spawn(async move { verified.run(&dir, tx, child).await });
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
        push(clean);
    }
    match join.await {
        Ok(Ok(status)) if status.success() => Ran::Finished { release },
        // the installer said why on its own stderr; a status alone would replace that
        Ok(Ok(status)) => {
            failed(last_stderr.unwrap_or_else(|| format!("the installer exited with {status}")))
        }
        Ok(Err(SatzError::Cancelled)) => Ran::Cancelled(CommandOutcome {
            ok: false,
            text: "cancelled — the installer was stopped where it stood".to_string(),
        }),
        Ok(Err(e)) => failed(e.to_string()),
        Err(e) => failed(format!("the install task failed: {e}")),
    }
}

/// What an installer of `release` that exited cleanly left. A satz the search finds is the
/// outcome; one it still cannot use is a failure naming why — an exit status of zero is not
/// the proof.
fn found_after_install(status: &SatzStatus, release: &str) -> CommandOutcome {
    let failed = |text: String| CommandOutcome { ok: false, text };
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
        SatzStatus::TooOld { found, .. } => failed(format!(
            "the installer of {release} finished, but the satz found is {found}, which is too old"
        )),
        SatzStatus::Missing(why) | SatzStatus::Unusable(why) => failed(format!(
            "the installer of {release} finished, but satz is still not usable: {why}"
        )),
        SatzStatus::Unknown => failed(format!(
            "the installer of {release} finished, and satz has not been located since"
        )),
    }
}

/// What an update by the installer of `release` left. It is done when the satz the search
/// now finds IS that release: the installer writes `satz.exe` beside the satz in use, and a
/// satz Settings name under another file name stays what it was, which its version says.
fn found_after_update(status: &SatzStatus, release: &str) -> CommandOutcome {
    match status {
        SatzStatus::Located(bin) if bin.version.to_string() == release.trim_start_matches('v') => {
            CommandOutcome {
                ok: true,
                text: format!("satz updated to {}", bin.version),
            }
        }
        SatzStatus::Located(bin) => CommandOutcome {
            ok: false,
            text: format!(
                "the installer of {release} finished, but the satz in use at {} is still {}",
                bin.path.display(),
                bin.version
            ),
        },
        _ => found_after_install(status, release),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use satz_studio_core::satz::SatzBinary;

    use super::*;

    #[test]
    fn a_clean_exit_is_an_install_only_when_the_search_then_finds_a_satz() {
        let built = SatzBinary::built_against();
        let bin = SatzBinary::check(PathBuf::from("/opt/satz"), built).unwrap();
        let located = found_after_install(&SatzStatus::Located(bin), "v0.73.1");
        assert!(located.ok);
        assert!(located.text.contains("/opt/satz"), "{}", located.text);

        let missing = found_after_install(
            &SatzStatus::Missing("satz not found; tried satz (on PATH)".to_string()),
            "v0.73.1",
        );
        assert!(!missing.ok);
        assert!(
            missing.text.contains("still not usable"),
            "{}",
            missing.text
        );
    }

    #[test]
    fn an_update_by_the_installer_is_done_only_when_the_satz_in_use_is_that_release() {
        let built = SatzBinary::built_against();
        let release = format!("v{built}");
        let bin = SatzBinary::check(PathBuf::from("/opt/satz"), built.clone()).unwrap();
        let done = found_after_update(&SatzStatus::Located(bin), &release);
        assert!(done.ok, "{}", done.text);

        // a satz Settings name under another file name was not replaced
        let newer = semver::Version::new(built.major, built.minor, built.patch + 1);
        let stale = SatzBinary::check(PathBuf::from("/opt/satz-pinned"), built).unwrap();
        let said = found_after_update(&SatzStatus::Located(stale), &format!("v{newer}"));
        assert!(!said.ok);
        assert!(said.text.contains("/opt/satz-pinned"), "{}", said.text);
        assert!(said.text.contains("is still"), "{}", said.text);

        let too_old = found_after_update(
            &SatzStatus::TooOld {
                path: PathBuf::from("/opt/satz"),
                found: "0.1.0".to_string(),
                required: "0.73.0".to_string(),
            },
            &release,
        );
        assert!(!too_old.ok);
        assert!(too_old.text.contains("too old"), "{}", too_old.text);
    }

    #[test]
    fn the_log_header_names_this_system_s_installer_and_the_folder() {
        let dir = PathBuf::from("bin");
        let said = header(&dir);
        let installer = Installer::for_this_system();
        assert!(said.contains(installer.asset()), "{said}");
        assert!(said.contains("SATZ_INSTALL_DIR=bin"), "{said}");
        assert!(said.contains("SATZ_NO_MODIFY_PATH=1"), "{said}");
    }
}
