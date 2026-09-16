//! Edit primitives over the document layer, and the write discipline: the new text is
//! built in memory, satz-core's parser must accept it and the document tree must differ
//! only at the edited nodes, the temp file beside the real one is checked by
//! `satz transpile --check`, then the real file is replaced atomically. A file that
//! changed under the app is refused, never merged. Answers and pack toggles do not
//! come through here — they are satz's own writer, called through `satz_interview`;
//! [`Snapshot`] is the shape that write takes.
//!
//! The lock lives in the session: a caller holds [`EstateSession::write_lock`] across
//! [`EditSession::apply`] and [`Proposed::commit`], and across a delegated write and
//! its [`Snapshot::verify`], so one writer at a time reaches the file.
//!
//! [`EstateSession::write_lock`]: crate::satz::EstateSession::write_lock

mod apply;
pub mod check;
mod commit;
pub mod snapshot;

use std::path::{Path, PathBuf};
use std::pin::Pin;

pub use check::{CliChecker, McpChecker};
pub use snapshot::Snapshot;

use crate::cst::{Cst, NodeId, TypedValue};
use crate::diag::Diagnostic;
use crate::satz::SatzError;
use crate::satz::reports::CompileSummary;

#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// replace the value span of an attribute, a list item or an object attribute
    ReplaceValue { node: NodeId, value: TypedValue },
    /// replace a param's value; an absent param is appended before `}` as `bind` appends it
    ReplaceParam { name: String, value: TypedValue },
}

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("line {line}: {message}")]
    Syntax { line: u32, message: String },
    #[error("node {0} is not in this document")]
    NodeNotFound(NodeId),
    #[error("node {0} is not a value")]
    NotAValue(NodeId),
    #[error("the estate has no `params {{ }}` block — answers are written there")]
    NoParamsBlock,
    #[error("the edit changed the document beyond its target (line {line}) — refused")]
    ChangedElsewhere { line: u32 },
    #[error("{0}: not a `.satz` file — satz reads no other")]
    NotSatz(PathBuf),
    #[error("two edits target {0} — refused")]
    Duplicate(String),
    #[error("node {inner} lies inside node {outer}, and both are edited — refused")]
    Nested { inner: NodeId, outer: NodeId },
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Cst(#[from] crate::cst::CstError),
}

/// Why a commit did not land. In both cases the real file is exactly as it was.
#[derive(Debug)]
pub enum Rollback {
    /// the file on disk is not the file that was opened
    ChangedOnDisk,
    /// satz refused the temp file; the diagnostics name the real file
    Check(Vec<Diagnostic>),
}

#[derive(Debug, thiserror::Error)]
pub enum CommitError {
    #[error("not written: {0:?}")]
    Rollback(Rollback),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "{path}: the bytes on disk after the rename are not the bytes written — another writer is active"
    )]
    Overwritten { path: PathBuf },
    #[error(transparent)]
    Satz(#[from] SatzError),
}

/// What a check returned when it did not pass.
#[derive(Debug)]
pub enum CheckFailure {
    /// satz compiled the file and refused it — one diagnostic per finding, at the
    /// line the finding names
    Refused(Vec<Diagnostic>),
    /// satz could not be run at all, or answered in a shape this version does not read
    Failed(SatzError),
}

pub type CheckFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CompileSummary, CheckFailure>> + Send + 'a>>;

/// `satz transpile --check` over one estate file: the MCP session in the app, the CLI
/// in the verification harness. Both must agree. `estate` is absolute: satz resolves a
/// relative name inside `yaml_dir`, and a temp file is named by its own path.
pub trait Checker: Send + Sync {
    fn check<'a>(&'a self, estate: &'a Path) -> CheckFuture<'a>;
}

/// A file opened for editing: its bytes as they were, hashed, and its document tree.
#[derive(Debug, Clone)]
pub struct EditSession {
    path: PathBuf,
    original: String,
    sha256: String,
    cst: Cst,
}

/// An edit applied in memory, not yet on disk.
#[derive(Debug, Clone)]
pub struct Proposed {
    session: EditSession,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    pub path: PathBuf,
    /// the hash of what is on disk now — the next session's snapshot
    pub sha256: String,
    /// what the check reported: the emitted addresses, and the findings it did not
    /// refuse on — the warnings and notes the app shows at their lines after the write
    pub summary: CompileSummary,
}

impl EditSession {
    /// Read the file, hash its bytes and build its tree. The path is made absolute, so
    /// the temp file and the checkers are named by a path that does not depend on the
    /// working directory; a file without the `.satz` extension is refused, since satz
    /// reads no other.
    pub fn open(path: &Path) -> Result<EditSession, EditError> {
        if path.extension().and_then(|e| e.to_str()) != Some("satz") {
            return Err(EditError::NotSatz(path.to_path_buf()));
        }
        let path = std::path::absolute(path).map_err(|e| EditError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        let bytes = std::fs::read(&path).map_err(|e| EditError::Io {
            path: path.clone(),
            source: e,
        })?;
        let original = String::from_utf8(bytes).map_err(|e| EditError::Io {
            path: path.clone(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?;
        let sha256 = sha256_hex(original.as_bytes());
        let cst = Cst::parse(&original)?;
        Ok(EditSession {
            path,
            original,
            sha256,
            cst,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn text(&self) -> &str {
        &self.original
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn cst(&self) -> &Cst {
        &self.cst
    }
    /// Apply edits in memory: the result parses, and differs from the original only at
    /// the edited nodes.
    ///
    /// Each [`Edit::ReplaceValue`] names a `Value` node, or an `Attr` or `ParamEntry`
    /// whose value node is then the target; anything else is [`EditError::NotAValue`].
    /// The new text is [`render_value`](crate::cst::render_value) in the
    /// [`style_of`](crate::cst::style_of) the value node, spliced over the node's span
    /// and nothing else, so the `=` column and a trailing comment on the line stay as
    /// they are. [`Edit::ReplaceParam`] does the same on the entry `params { … }`
    /// binds; a param the block does not bind is appended before its `}` as satz's own
    /// `bind` appends it — `name = value` on its own line, several appends stacked in the
    /// order given — and the params block is then laid out as `satz fmt` lays it out,
    /// the rest of the file untouched, which is what `bind` does after an append: the
    /// block keeps one `=` column. Two edits on one node, an edit inside another edited
    /// node, and two appends of one name are refused. Splices land from the highest
    /// span start to the lowest, so every span of the original tree stays valid.
    ///
    /// The proof, in two parts. `satz_core::satz::parse` must accept the new text
    /// ([`EditError::Syntax`] otherwise). Then the new tree is compared with the old:
    /// both are walked in document order into a sequence of node signatures — the kind
    /// and, where the kind carries one, the key, name, path or gate text; a scalar
    /// value, a comment, an opaque statement and an error node by their whole text —
    /// where each edited value node stands as a hole (its subtree skipped, and in the
    /// new tree only a `Value` node with exactly the spliced span counts as that hole)
    /// and each appended param as one `ParamEntry` of that name inside the appended
    /// range. Any other difference is [`EditError::ChangedElsewhere`] naming the line
    /// of the first node in the new text that does not match. After an append the
    /// laid-out text is held to the same two parts once more, against the text before
    /// the layout, which may differ from it in whitespace only.
    pub fn apply(&self, edits: &[Edit]) -> Result<Proposed, EditError> {
        let text = apply::apply(&self.cst, edits)?;
        Ok(Proposed {
            session: self.clone(),
            text,
        })
    }
}

impl Proposed {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn session(&self) -> &EditSession {
        &self.session
    }
    /// The write discipline, start to finish:
    ///
    /// 1. the file is read and hashed; a hash that is not the session's is
    ///    [`Rollback::ChangedOnDisk`] — no merge;
    /// 2. the new text is written to `<stem>.studio-tmp.satz` beside the file, so
    ///    `use "…"` and `include_dirs` resolve as they do for the file, and the name
    ///    keeps the extension satz reads;
    /// 3. `checker.check(tmp)`: a refusal deletes the temp file and is
    ///    [`Rollback::Check`] with every diagnostic re-pointed from the temp path (as
    ///    given and canonical) to the real one; a checker that could not run deletes it
    ///    and is [`CommitError::Satz`];
    /// 4. the temp file is renamed over the real one;
    /// 5. the file is read again and hashed: [`Committed`] carries that hash and the
    ///    summary — its `estate` re-pointed to the real path, its `findings` what the
    ///    check reported without refusing.
    ///
    /// Every I/O failure names its path. The temp file never survives a failure: when
    /// it cannot be removed, the error says so beside the failure that came first.
    pub async fn commit(self, checker: &dyn Checker) -> Result<Committed, CommitError> {
        commit::commit(self, checker).await
    }
}

/// The sha256 of a file's bytes, hex — the snapshot every commit compares against.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(bytes))
}
