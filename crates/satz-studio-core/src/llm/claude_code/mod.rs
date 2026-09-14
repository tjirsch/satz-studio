//! Claude Code as a chat backend: the app's second engine, for a claude.ai
//! subscription rather than an API key.
//!
//! The app never reads Claude Code's credential. It drives the installed CLI —
//! `claude -p --input-format stream-json --output-format stream-json` — over stdio
//! with the control protocol, gives it the open estate's own satz MCP server and
//! nothing else, answers its permission requests with the app's approval card, and
//! holds the estate's write lock for the turn. What the CLI streams back is the
//! Messages API's own event stream, so the Chat view sees the same [`AgentEvent`]s it
//! sees from the API backend.
//!
//! - [`cli`] — where the binary is, which version, whether it is signed in.
//! - [`events`] — the lines the CLI writes, typed.
//! - [`session`] — one process per estate: the command line, the turn, the approval
//!   round trip, the interrupt.
//!
//! [`AgentEvent`]: crate::llm::AgentEvent

pub mod cli;
pub mod events;
pub mod session;

pub use cli::{AuthStatus, ClaudeCodeCli, ClaudeCodeError};
pub use events::{CcLine, RateLimit};
pub use session::{
    Session, SessionOptions, allowed_tools, command_args, mcp_config, system_prompt,
};
