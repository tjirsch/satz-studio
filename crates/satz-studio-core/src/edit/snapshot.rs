//! The delegated-write shape: a write satz performs itself (`satz_interview` binding
//! an answer, uncommenting a pack line) lands on the real file, so the discipline
//! runs around it — the bytes are recorded first, and whatever the call comes to, the
//! file afterwards is either one the check passed or the recorded bytes. A call that
//! landed is checked on the real path and a refusal writes the recorded bytes back; a
//! call satz refused, or one that never returned, is compared byte for byte with the
//! record and written back when it differs, because a tool that refuses has not
//! necessarily written nothing. "Rolled back" means the bytes are back.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{CheckFailure, Checker, CommitError, Committed, McpChecker, Rollback, sha256_hex};
use crate::satz::{EstateSession, SatzError, ToolOutcome};

/// A file's bytes before a delegated write, with their hash.
#[derive(Debug, Clone)]
pub struct Snapshot {
    path: PathBuf,
    bytes: Vec<u8>,
    sha256: String,
}

/// What a delegated write came to. In every arm the file on disk is either what a
/// passing check saw or the recorded bytes, and the arm says which.
#[derive(Debug)]
pub enum Delegated {
    /// the call landed and the check passed
    Landed {
        outcome: ToolOutcome,
        committed: Committed,
    },
    /// the call landed and the check refused the file, or could not run: the recorded
    /// bytes are back
    RolledBack {
        outcome: ToolOutcome,
        error: CommitError,
    },
    /// the call did not land
    NotLanded(NotLanded),
}

/// A call that did not land, and what became of the file.
#[derive(Debug)]
pub struct NotLanded {
    pub cause: Cause,
    pub restore: Restore,
}

/// Why a call did not land.
#[derive(Debug)]
pub enum Cause {
    /// satz refused it: the outcome with `is_error`, its text satz's sentence
    Refused(ToolOutcome),
    /// it returned no result — the session died, or the server answered with an error
    /// in place of one
    Failed(SatzError),
}

/// The file after a call that did not land, compared with the record.
#[derive(Debug)]
pub enum Restore {
    /// the bytes on disk are the recorded ones
    Untouched,
    /// the bytes differed — or the file was gone — and the recorded ones are back
    Restored(PathBuf),
    /// the bytes differed and could not be written back
    Failed(CommitError),
}

impl Snapshot {
    /// Record the file as it is. The path is made absolute, so the checker is named a
    /// path that does not depend on the working directory.
    pub fn take(path: &Path) -> Result<Snapshot, CommitError> {
        let path = std::path::absolute(path).map_err(|e| CommitError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        let bytes = std::fs::read(&path).map_err(|e| CommitError::Io {
            path: path.clone(),
            source: e,
        })?;
        let sha256 = sha256_hex(&bytes);
        Ok(Snapshot {
            path,
            bytes,
            sha256,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// After the delegated write: check the real path. A pass is [`Committed`] with
    /// the hash of what is on disk now. A refusal writes the recorded bytes back and is
    /// [`Rollback::Check`]; a checker that could not run writes them back too and is
    /// [`CommitError::Satz`] — a write nothing verified does not stand.
    pub async fn verify(self, checker: &dyn Checker) -> Result<Committed, CommitError> {
        let Snapshot { path, bytes, .. } = self;
        let io = |e: std::io::Error| CommitError::Io {
            path: path.clone(),
            source: e,
        };
        let failure = match checker.check(&path).await {
            Ok(summary) => {
                let now = std::fs::read(&path).map_err(io)?;
                return Ok(Committed {
                    path,
                    sha256: sha256_hex(&now),
                    summary,
                });
            }
            Err(CheckFailure::Refused(diags)) => CommitError::Rollback(Rollback::Check(diags)),
            Err(CheckFailure::Failed(e)) => CommitError::Satz(e),
        };
        std::fs::write(&path, &bytes).map_err(io)?;
        Err(failure)
    }

    /// After a call that did not land: the file compared byte for byte with the record,
    /// and the recorded bytes written back when it differs. A file that is gone counts
    /// as changed.
    pub fn restore_if_changed(&self) -> Restore {
        match std::fs::read(&self.path) {
            Ok(now) if now == self.bytes => return Restore::Untouched,
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Restore::Failed(CommitError::Io {
                    path: self.path.clone(),
                    source: e,
                });
            }
        }
        match std::fs::write(&self.path, &self.bytes) {
            Ok(()) => Restore::Restored(self.path.clone()),
            Err(e) => Restore::Failed(CommitError::Io {
                path: self.path.clone(),
                source: e,
            }),
        }
    }

    /// The whole delegated write around `call`, the tool call on the session, which
    /// runs only once this is awaited — after the record was taken. A call that landed
    /// is [`Snapshot::verify`]'d; a refusal and a call that returned nothing go through
    /// [`Snapshot::restore_if_changed`]. The caller holds the write lock across it.
    pub async fn delegate<F>(self, call: F, checker: &dyn Checker) -> Delegated
    where
        F: Future<Output = Result<ToolOutcome, SatzError>>,
    {
        match call.await {
            Ok(outcome) if outcome.is_error => Delegated::NotLanded(NotLanded {
                cause: Cause::Refused(outcome),
                restore: self.restore_if_changed(),
            }),
            Ok(outcome) => match self.verify(checker).await {
                Ok(committed) => Delegated::Landed { outcome, committed },
                Err(error) => Delegated::RolledBack { outcome, error },
            },
            Err(error) => Delegated::NotLanded(NotLanded {
                cause: Cause::Failed(error),
                restore: self.restore_if_changed(),
            }),
        }
    }
}

/// The whole of a delegated write on the estate's main file, as the app makes it: the
/// session's write lock, the bytes recorded, then `call` — a tool over the session, or a
/// satz command through its CLI ([`crate::satz::project::add_project`]) — inside
/// [`Snapshot::delegate`], with [`McpChecker`] for the check of a call that landed.
/// `call` runs only once the lock is held and the record taken. A file that cannot be
/// recorded is the error, and nothing ran.
pub async fn delegated_write<F>(
    session: &Arc<EstateSession>,
    call: F,
) -> Result<Delegated, CommitError>
where
    F: Future<Output = Result<ToolOutcome, SatzError>>,
{
    let _lock = session.write_lock().await;
    let snapshot = Snapshot::take(&session.main)?;
    let checker = McpChecker {
        session: Arc::clone(session),
    };
    Ok(snapshot.delegate(call, &checker).await)
}

impl NotLanded {
    /// What the operator reads, for the toast and the drawer: satz's refusal, or the
    /// call's error under the tool's name, followed by what became of the file —
    /// nothing more when satz had left it as it was.
    pub fn message(&self, tool: &str) -> String {
        let (text, who) = match &self.cause {
            Cause::Refused(outcome) => (outcome.text.clone(), "satz refused"),
            Cause::Failed(error) => (
                format!("{tool}: {error}"),
                "the call ended without a result",
            ),
        };
        match &self.restore {
            Restore::Untouched => text,
            Restore::Restored(path) => format!(
                "{text} — {who} and had changed {}; the file is back as it was",
                file_name(path)
            ),
            Restore::Failed(e) => format!(
                "{text} — {who} and had changed the file, and it could not be put back: {e}"
            ),
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
