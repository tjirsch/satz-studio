//! git, as far as the app has to know it. `satz merge-presets` edits the estate file in
//! place and proves the edit has an undo by asking git (`git status --porcelain` in the
//! estate file's directory, satz `src/presets.rs`, `is_git_dirty`): outside a work tree,
//! or with no git to ask, it refuses. `satz init` writes a `.gitignore` and makes no
//! repository, so an estate the app creates or imports cannot take a preset update
//! until someone makes one.
//!
//! [`WorkTree::read`] asks git the same question, in the same directory, before satz
//! does; [`init_steps`] are the commands that make the repository, which the app runs
//! only when the operator asks, through [`run`].

use std::ffi::OsStr;
use std::path::Path;
use std::process::{ExitStatus, Stdio};

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::satz::CliLine;

/// Whether git holds a directory in a work tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkTree {
    /// `git rev-parse --is-inside-work-tree` answered `true`: the directory is in a
    /// repository's work tree — its own, or one above it
    Inside,
    /// git ran and did not answer `true`; its own words
    Outside(String),
    /// git could not be run at all — not installed, or not on the PATH
    NoGit(String),
}

impl WorkTree {
    /// Ask git in `dir`. A repository ABOVE `dir` counts: an estate inside a larger
    /// repository has its undo there.
    pub async fn read(dir: &Path) -> WorkTree {
        WorkTree::ask(OsStr::new("git"), dir).await
    }

    async fn ask(git: &OsStr, dir: &Path) -> WorkTree {
        // a directory that is not there would fail the spawn, which reads as no git
        if !dir.is_dir() {
            return WorkTree::Outside(format!("{}: not a directory", dir.display()));
        }
        let output = Command::new(git)
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(dir)
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await;
        let output = match output {
            Ok(o) => o,
            Err(e) => return WorkTree::NoGit(format!("running git: {e}")),
        };
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if output.status.success() && stdout == "true" {
            return WorkTree::Inside;
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        WorkTree::Outside(match (stderr.is_empty(), stdout.is_empty()) {
            (false, _) => stderr,
            (true, false) => format!("git rev-parse --is-inside-work-tree: {stdout}"),
            (true, true) => format!("git rev-parse exited with {}", output.status),
        })
    }
}

/// The commands that put an estate directory into a repository of its own with one
/// commit: `git init -b main`, `git add -A`, and a commit whose message names the
/// estate. The commit takes git's configured identity; the app sets none.
pub fn init_steps(estate: &str) -> [Vec<String>; 3] {
    [
        vec!["init".to_string(), "-b".to_string(), "main".to_string()],
        vec!["add".to_string(), "-A".to_string()],
        vec![
            "commit".to_string(),
            "-m".to_string(),
            format!("the estate {estate} as it stands"),
        ],
    ]
}

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("cancelled")]
    Cancelled,
}

/// Run `git <args…>` in `dir`, streaming every line of both pipes into `out`; the exit
/// status is the result, and a refusal is git's own lines in `out` and a status that is
/// not success. `cancel` kills the child and the result is [`GitError::Cancelled`].
pub async fn run(
    dir: &Path,
    args: &[String],
    out: mpsc::Sender<CliLine>,
    cancel: CancellationToken,
) -> Result<ExitStatus, GitError> {
    let command = format!("git {}", args.join(" "));
    let mut child = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| GitError::Io {
            context: format!("spawning `{command}`"),
            source: e,
        })?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let pump_out = tokio::spawn(pump(stdout, out.clone(), CliLine::Stdout));
    let pump_err = tokio::spawn(pump(stderr, out, CliLine::Stderr));
    let status = tokio::select! {
        () = cancel.cancelled() => {
            if let Err(e) = child.kill().await {
                // `kill` refuses a child that has already exited; anything else is a
                // real failure
                let exited = child.try_wait().map_err(|e| GitError::Io { context: format!("waiting for `{command}`"), source: e })?.is_some();
                if !exited {
                    return Err(GitError::Io { context: format!("killing `{command}`"), source: e });
                }
            }
            return Err(GitError::Cancelled);
        }
        status = child.wait() => status.map_err(|e| GitError::Io {
            context: format!("waiting for `{command}`"),
            source: e,
        })?,
    };
    for handle in [pump_out, pump_err] {
        handle
            .await
            .map_err(|e| GitError::Io {
                context: format!("reading the output of `{command}`"),
                source: std::io::Error::other(e),
            })?
            .map_err(|e| GitError::Io {
                context: format!("reading the output of `{command}`"),
                source: e,
            })?;
    }
    Ok(status)
}

/// Forward one pipe line by line; once the receiver is gone the lines are read and
/// dropped, so the child never blocks on a full pipe.
async fn pump<R: AsyncRead + Unpin>(
    pipe: R,
    out: mpsc::Sender<CliLine>,
    wrap: fn(String) -> CliLine,
) -> std::io::Result<()> {
    let mut lines = BufReader::new(pipe).lines();
    let mut forwarding = true;
    while let Some(line) = lines.next_line().await? {
        if forwarding && out.send(wrap(line)).await.is_err() {
            forwarding = false;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_git_that_cannot_be_run_is_no_git_and_a_missing_directory_is_not_mistaken_for_one() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            WorkTree::ask(OsStr::new("git-that-satz-studio-never-finds"), dir.path()).await,
            WorkTree::NoGit(_)
        ));
        let gone = dir.path().join("gone");
        assert!(matches!(
            WorkTree::ask(OsStr::new("git-that-satz-studio-never-finds"), &gone).await,
            WorkTree::Outside(said) if said.ends_with("not a directory")
        ));
    }
}
