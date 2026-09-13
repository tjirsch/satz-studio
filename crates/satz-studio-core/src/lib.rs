//! satz-studio-core — the headless half of satz-studio.
//!
//! Everything the desktop app knows lives here and is testable without a window:
//!
//! - [`estate`] — an estate directory: its `config.toml`, the `.satz` files beside it,
//!   the loader that resolves `use "…"` the way satz resolves it.
//! - [`cst`] — the lossless document layer over the Satz grammar (tree-sitter, vendored
//!   from `satz-tree-sitter`): byte spans, comments kept, `text()` is the file.
//! - [`edit`] — the edit primitives and the write discipline: a value is replaced by
//!   span, the temp file is checked by satz, the real file is replaced atomically.
//! - [`schema`] and [`model`] — the provider schema and the view model the app binds to.
//! - [`satz`] — the driver: the binary and its version gate, the CLI runner, the MCP
//!   session over `satz mcp`, one session per estate.
//! - [`llm`] — the Claude client (Messages API over HTTPS), the agent loop, the
//!   provider adapters, credentials; [`transcript`] keeps the conversations.
//! - [`settings`] and [`diag`] — the settings file and the one diagnostic type.
//!
//! The public surface of every module is the contract between the units that build
//! satz-studio in parallel (see `docs/architecture.md`). A function whose unit has
//! not shipped returns [`Unimplemented`] rather than panicking.

pub mod cst;
pub mod diag;
pub mod edit;
pub mod estate;
pub mod llm;
pub mod model;
pub mod satz;
pub mod schema;
pub mod settings;
pub mod transcript;

/// A stub left by the scaffold: the unit that owns this function has not shipped yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{what} is not built yet (satz-studio unit {unit})")]
pub struct Unimplemented {
    pub what: &'static str,
    pub unit: &'static str,
}

impl Unimplemented {
    pub const fn new(what: &'static str, unit: &'static str) -> Self {
        Self { what, unit }
    }
}
