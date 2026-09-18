//! Claude, natively: the Messages API over HTTPS, the wire types as the app's own
//! message model, the SSE stream, the agent loop with the satz tools bridged from the
//! MCP session, credentials, and the adapters that map other providers INTO this
//! model. Claude never passes through a mapping.
//!
//! - [`claude`] — the wire types, the SSE decoder and assembler, the client, the error.
//! - [`agent`] — the loop, the approval gate, the tool bridge, the tool host.
//! - [`auth`] — where a credential comes from.
//! - [`provider`] — the OpenAI-compatible and Ollama adapters.
//! - [`claude_code`] — the second engine: the installed Claude Code CLI driven over
//!   stdio, for a claude.ai subscription instead of an API key (ADR 0010). It raises
//!   the same [`AgentEvent`]s, so the Chat view consumes one stream from either.
//!
//! Every public type is reachable here; the modules are the reading order.

use std::pin::Pin;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub mod agent;
pub mod auth;
pub mod claude;
pub mod claude_code;
pub mod provider;

pub use agent::bridge::{result_text, tool_defs, tool_result};
pub use agent::{Agent, AgentEvent, Approval, EstateContext, MAX_TOKENS, ToolHost};
pub use auth::{Credential, CredentialSource};
pub use claude::client::ClaudeClient;
pub use claude::error::ClaudeError;
pub use claude::types::{
    CacheControl, ContentBlock, Effort, Message, Request, Response, Role, StopDetails, StopReason,
    SystemBlock, ToolDef, Usage,
};
pub use claude_code::{AuthStatus, ClaudeCodeCli, ClaudeCodeError, SessionOptions};
pub use provider::ollama::Ollama;
pub use provider::openai_compat::OpenAiCompat;

/// What a stream yields as it arrives, provider-agnostic. The deltas are for display;
/// [`StreamEvent::BlockStop`] carries every block complete — a tool's input parsed, a
/// thinking block with its signature, an unknown block verbatim — and a consumer builds
/// the response from those and from [`StreamEvent::Done`].
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Started {
        id: String,
        model: String,
    },
    TextDelta(String),
    ThinkingDelta(String),
    ToolUseStart {
        index: usize,
        id: String,
        name: String,
    },
    ToolInputDelta {
        index: usize,
        partial_json: String,
    },
    BlockStop {
        index: usize,
        block: ContentBlock,
    },
    Done {
        stop_reason: StopReason,
        stop_details: Option<StopDetails>,
        usage: Usage,
    },
    Error(String),
}

/// What a provider can do; the Chat view shows what the chosen one lacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub tools: bool,
    pub thinking: bool,
    pub effort: bool,
    pub cache_control: bool,
}

pub type StreamFuture<'a> = Pin<Box<dyn Future<Output = Result<(), ClaudeError>> + Send + 'a>>;

/// A chat provider. Claude is the native one; the others map [`Request`] into their
/// wire format and their stream back into [`StreamEvent`]s. `stream` sends the events
/// through `tx` and returns when the stream ends; an error is also sent as
/// [`StreamEvent::Error`] before it is returned.
pub trait ChatProvider: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    fn stream<'a>(
        &'a self,
        req: &'a Request,
        tx: mpsc::Sender<StreamEvent>,
        cancel: CancellationToken,
    ) -> StreamFuture<'a>;
}
