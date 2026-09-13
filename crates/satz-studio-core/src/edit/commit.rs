//! The write discipline: the hash on disk must be the session's, the new text goes to
//! a temp file beside the real one, the checker judges the temp file, the rename lands
//! it. A refusal leaves the real file untouched and no temp file behind.

use std::path::{Path, PathBuf};

use super::{CheckFailure, Checker, CommitError, Committed, Proposed, Rollback, sha256_hex};

pub(super) async fn commit(
    proposed: Proposed,
    checker: &dyn Checker,
) -> Result<Committed, CommitError> {
    let Proposed { session, text } = proposed;
    let path = session.path().to_path_buf();
    let on_disk = read(&path)?;
    if sha256_hex(&on_disk) != session.sha256() {
        return Err(CommitError::Rollback(Rollback::ChangedOnDisk));
    }
    let tmp = temp_path(&path);
    std::fs::write(&tmp, text.as_bytes()).map_err(|e| io(&tmp, e))?;
    let canonical = match tmp.canonicalize() {
        Ok(c) => c,
        Err(e) => return Err(discard(&tmp, io(&tmp, e))),
    };
    let mut summary = match checker.check(&tmp).await {
        Ok(s) => s,
        Err(CheckFailure::Refused(diags)) => {
            let diags = diags
                .into_iter()
                .map(|d| d.repoint(&tmp, &path).repoint(&canonical, &path))
                .collect();
            return Err(discard(&tmp, CommitError::Rollback(Rollback::Check(diags))));
        }
        Err(CheckFailure::Failed(e)) => return Err(discard(&tmp, CommitError::Satz(e))),
    };
    if let Err(e) = std::fs::rename(&tmp, &path) {
        return Err(discard(&tmp, io(&path, e)));
    }
    let bytes = read(&path)?;
    let sha256 = sha256_hex(&bytes);
    if sha256 != sha256_hex(text.as_bytes()) {
        return Err(CommitError::Overwritten { path });
    }
    if Path::new(&summary.estate) == tmp || Path::new(&summary.estate) == canonical {
        summary.estate = path.display().to_string();
    }
    Ok(Committed {
        path,
        sha256,
        summary,
    })
}

/// `<stem>.studio-tmp.satz` beside `path`: the same directory, so `use "…"` and
/// `include_dirs` resolve as they do for the file, and the extension satz reads.
pub(super) fn temp_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .expect("a `.satz` path has a stem")
        .to_string_lossy();
    path.with_file_name(format!("{stem}.studio-tmp.satz"))
}

fn read(path: &Path) -> Result<Vec<u8>, CommitError> {
    std::fs::read(path).map_err(|e| io(path, e))
}

fn io(path: &Path, source: std::io::Error) -> CommitError {
    CommitError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Remove the temp file after `first` went wrong; when even that fails, the error
/// carries both.
fn discard(tmp: &Path, first: CommitError) -> CommitError {
    match std::fs::remove_file(tmp) {
        Ok(()) => first,
        Err(e) => CommitError::Io {
            path: tmp.to_path_buf(),
            source: std::io::Error::other(format!(
                "{first}; and the temp file could not be removed: {e}"
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_temp_file_keeps_the_extension_and_the_directory() {
        assert_eq!(
            temp_path(Path::new("/e/yaml/acme.satz")),
            PathBuf::from("/e/yaml/acme.studio-tmp.satz")
        );
        assert_eq!(
            temp_path(Path::new("/e/yaml/acme.local.satz")),
            PathBuf::from("/e/yaml/acme.local.studio-tmp.satz")
        );
    }
}
