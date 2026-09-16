//! The satz driver: the binary and its version gate ([`binary`]), the CLI runner
//! ([`cli`]), `satz init` and what it leaves behind ([`init`]), `satz import` and what
//! it wrote and found ([`import`]), the MCP session over
//! `satz mcp` ([`mcp`]), and one session per open estate that the command decks and the
//! agent share ([`session`]). [`reports`] are the JSON payloads a reporting command
//! writes with `--format json` and the server returns as `structuredContent`.

pub mod binary;
pub mod cli;
pub mod import;
pub mod init;
pub mod mcp;
pub mod reports;
pub mod session;

pub use binary::{MIN_SATZ, SatzBinary};
pub use cli::{CliLine, SatzCli};
pub use import::{ImportOptions, ImportPlan, ImportReport, ImportShape};
pub use init::InitOptions;
pub use mcp::{McpSession, ToolAnnotations, ToolInfo, ToolOutcome};
pub use session::EstateSession;

use std::path::PathBuf;

/// The capability ceiling a `satz mcp` is started with (`--allow`). The server never
/// exceeds it; `exec` lets tools run external programs (Checkov).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Allow {
    Read,
    #[default]
    ReadWrite,
    ReadWriteExec,
}

impl Allow {
    /// The value satz takes after `--allow`.
    pub fn as_arg(self) -> &'static str {
        match self {
            Allow::Read => "read",
            Allow::ReadWrite => "read,write",
            Allow::ReadWriteExec => "read,write,exec",
        }
    }
    pub fn writes(self) -> bool {
        !matches!(self, Allow::Read)
    }
}

impl std::fmt::Display for Allow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_arg())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SatzError {
    #[error("satz not found; tried {}", tried.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "))]
    NotFound { tried: Vec<PathBuf> },
    #[error(
        "satz {found} at {} is too old: satz-studio needs {required} or newer",
        path.display()
    )]
    TooOld {
        /// the binary that was found and refused — kept so the app can offer to update
        /// the very binary it is refusing, which is the only thing the operator can do
        /// about it from inside a window
        path: PathBuf,
        found: semver::Version,
        required: semver::Version,
    },
    #[error("could not read a version from `satz --version`: {0:?}")]
    VersionUnparsable(String),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("`satz {command}` exited with {status}:\n{stderr}")]
    Exit {
        command: String,
        status: std::process::ExitStatus,
        stderr: String,
    },
    #[error("`satz {command}` answered with JSON this app does not understand: {source}")]
    Json {
        command: String,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "the estate's directories share no common root, so `satz mcp` cannot be confined to one: {}. On Windows this is an estate and a directory its config names — presets, schemas, an include — sitting on different drives; they have to be on one.",
        dirs.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
    )]
    NoCommonRoot { dirs: Vec<PathBuf> },
    #[error("{0}: no such directory — a new estate is created in a directory that exists")]
    TargetMissing(PathBuf),
    #[error("{0}: already holds a config.toml — open that estate instead of creating one over it")]
    AlreadyAnEstate(PathBuf),
    #[error("{0}: no such file — an import reads a source that is there")]
    SourceMissing(PathBuf),
    #[error(
        "{0}: not a `tofu show -json` document (no `values.root_module`) — a raw .tfstate? run `tofu show -json > state.json`"
    )]
    NotAShowDocument(PathBuf),
    #[error(
        "`{0}` is not a scope: a live import takes organizations/<number>, folders/<number> or projects/<id>, or nothing at all for the import config's root"
    )]
    NotAScope(String),
    #[error("satz mcp: {0}")]
    Mcp(String),
    #[error("{tool} refused: {text}")]
    Refused { tool: String, text: String },
    #[error("the session is closed (satz mcp exited: {0})")]
    Closed(String),
    #[error("cancelled")]
    Cancelled,
}
