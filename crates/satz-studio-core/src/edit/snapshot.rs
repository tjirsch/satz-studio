//! The delegated-write shape: a write satz performs itself (`satz_interview` binding
//! an answer, uncommenting a pack line) lands on the real file, so the discipline
//! runs around it — the bytes are recorded first, the check runs on the real path
//! afterwards, and a refusal writes the recorded bytes back. "Rolled back" means the
//! bytes are back.

use std::path::{Path, PathBuf};

use super::{CheckFailure, Checker, CommitError, Committed, Rollback, sha256_hex};

/// A file's bytes before a delegated write, with their hash.
#[derive(Debug, Clone)]
pub struct Snapshot {
    path: PathBuf,
    bytes: Vec<u8>,
    sha256: String,
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
}
