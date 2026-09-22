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
//! - [`git`] — whether git holds the estate in a work tree, which `satz merge-presets`
//!   needs for its undo, and the commands that put it in one.
//! - [`schema`] and [`model`] — the provider schema and the view model the app binds to.
//! - [`satz`] — the driver: the binary and its version gate, the CLI runner, satz's
//!   installer verified before it runs, the MCP session over `satz mcp`, one session per
//!   estate.
//! - [`github`] — the latest release of a repository: the look for a newer satz-studio,
//!   and the release satz's installer is taken from.
//! - [`agent`] — the agentic client the window starts on the open estate; the
//!   configuration that points it there is satz's, printed by `satz mcp-config`
//!   ([`satz::mcp_config`]). satz-studio runs no model (ADR 0020).
//! - [`settings`] and [`diag`] — the settings file and the one diagnostic type.
//!
//! The public surface of every module is the contract between the crates and the
//! views (see `docs/architecture.md`).

pub mod agent;
pub mod cst;
pub mod diag;
pub mod edit;
pub mod estate;
pub mod git;
pub mod github;
pub mod model;
pub mod satz;
pub mod schema;
pub mod settings;
