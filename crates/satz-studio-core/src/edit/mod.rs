//! Edit primitives over the document layer, and the write discipline: the new text is
//! built in memory, satz-core's parser must accept it and the AST must differ only at
//! the edited node, the temp file beside the real one is checked by
//! `satz transpile --check`, then the real file is replaced atomically. A file that
//! changed under the app is refused, never merged. Answers and pack toggles do not
//! come through here — they are satz's own writer, called through `satz_interview`.

use std::path::{Path, PathBuf};
use std::pin::Pin;

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
    #[error("{path}: {source}")]
    Io { path: PathBuf, #[source] source: std::io::Error },
    #[error(transparent)]
    Cst(#[from] crate::cst::CstError),
    #[error(transparent)]
    Unimplemented(#[from] crate::Unimplemented),
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
    Io { path: PathBuf, #[source] source: std::io::Error },
    #[error(transparent)]
    Satz(#[from] SatzError),
    #[error(transparent)]
    Unimplemented(#[from] crate::Unimplemented),
}

/// What a check returned when it did not pass.
#[derive(Debug)]
pub enum CheckFailure {
    /// satz compiled the file and refused it — diagnostics with lines
    Refused(Vec<Diagnostic>),
    /// satz could not be run at all
    Failed(SatzError),
}

pub type CheckFuture<'a> = Pin<Box<dyn Future<Output = Result<CompileSummary, CheckFailure>> + Send + 'a>>;

/// `satz transpile --check` over one estate file: the MCP session in the app, the CLI
/// in the verification harness. Both must agree.
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
    pub summary: CompileSummary,
}

impl EditSession {
    pub fn open(path: &Path) -> Result<EditSession, EditError> {
        let _ = path;
        Err(crate::Unimplemented::new("EditSession::open", "U3").into())
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
    pub fn apply(&self, edits: &[Edit]) -> Result<Proposed, EditError> {
        let _ = edits;
        Err(crate::Unimplemented::new("EditSession::apply", "U3").into())
    }
}

impl Proposed {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn session(&self) -> &EditSession {
        &self.session
    }
    /// The write discipline, start to finish.
    pub async fn commit(self, checker: &dyn Checker) -> Result<Committed, CommitError> {
        let _ = checker;
        Err(crate::Unimplemented::new("Proposed::commit", "U3").into())
    }
}

/// The sha256 of a file's bytes, hex — the snapshot every commit compares against.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(bytes))
}
