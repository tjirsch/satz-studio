//! Claude, natively: the Messages API over HTTPS, the wire types as the app's own
//! message model, the SSE stream, the agent loop with the satz tools bridged from the
//! MCP session, credentials, and the adapters that map other providers INTO this
//! model. Claude never passes through a mapping.

use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::satz::{EstateSession, ToolInfo, ToolOutcome};

/// `output_config.effort`
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub kind: String,
}

impl CacheControl {
    pub fn ephemeral() -> Self {
        Self { kind: "ephemeral".to_string() }
    }
}

/// The content blocks of the Messages API. `Other` keeps anything newer verbatim, so a
/// transcript replays what the API returned.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    Thinking {
        thinking: String,
        #[serde(default)]
        signature: String,
    },
    RedactedThinking {
        data: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    Refusal,
    PauseTurn,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct StopDetails {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(default)]
    pub recommended_model: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u64>,
}

/// One request to `POST /v1/messages` — the app's chat request, in Claude's shape.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Request {
    pub model: String,
    pub max_tokens: u32,
    pub system: Vec<SystemBlock>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub effort: Effort,
    /// server-side refusal fallbacks (`fallbacks: "default"`)
    pub fallbacks: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SystemBlock {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// One assembled response.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Response {
    pub id: String,
    pub model: String,
    pub content: Vec<ContentBlock>,
    pub stop_reason: StopReason,
    #[serde(default)]
    pub stop_details: Option<StopDetails>,
    #[serde(default)]
    pub usage: Usage,
}

/// What a stream yields as it arrives, provider-agnostic.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Started { id: String, model: String },
    TextDelta(String),
    ThinkingDelta(String),
    ToolUseStart { index: usize, id: String, name: String },
    ToolInputDelta { index: usize, partial_json: String },
    BlockStop { index: usize },
    Done { stop_reason: StopReason, stop_details: Option<StopDetails>, usage: Usage },
    Error(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ClaudeError {
    #[error("bad request: {message}")]
    BadRequest { message: String, request_id: Option<String> },
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("permission denied: {0}")]
    Permission(String),
    #[error("model not found: {0}")]
    NotFound(String),
    #[error("request too large")]
    RequestTooLarge,
    #[error("rate limited{}", retry_after.map(|d| format!(", retry after {}s", d.as_secs())).unwrap_or_default())]
    RateLimited { retry_after: Option<std::time::Duration> },
    #[error("the API is overloaded")]
    Overloaded,
    #[error("server error {status}")]
    Server { status: u16 },
    #[error("connection: {0}")]
    Connection(String),
    #[error("stream: {0}")]
    Stream(String),
    #[error("refused{}{}", category.as_deref().map(|c| format!(" ({c})")).unwrap_or_default(), explanation.as_deref().map(|e| format!(": {e}")).unwrap_or_default())]
    Refused { category: Option<String>, explanation: Option<String>, recommended_model: Option<String> },
    #[error("no credential: set ANTHROPIC_API_KEY, or ANTHROPIC_AUTH_TOKEN, or log in with `ant auth login`, or enter a key in Settings")]
    NoCredential,
    #[error("cancelled")]
    Cancelled,
    #[error(transparent)]
    Unimplemented(#[from] crate::Unimplemented),
}

/// How a request authenticates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    /// `x-api-key`
    ApiKey(String),
    /// `Authorization: Bearer` + the OAuth beta header — from `ANTHROPIC_AUTH_TOKEN` or `ant auth`
    Bearer(String),
}

/// Where a credential came from — shown in Settings so a stale key is visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    ApiKeyEnv,
    AuthTokenEnv,
    AntProfile,
    Keychain,
}

impl Credential {
    /// `ANTHROPIC_API_KEY`, then `ANTHROPIC_AUTH_TOKEN`, then the `ant auth login`
    /// profile, then the keychain entry the Settings view writes. First hit wins.
    pub async fn resolve() -> Result<(Credential, CredentialSource), ClaudeError> {
        Err(crate::Unimplemented::new("Credential::resolve", "U6").into())
    }
    /// Store a key in the OS keychain (`satz-studio` / `anthropic-api-key`).
    pub fn store_in_keychain(key: &str) -> Result<(), ClaudeError> {
        let _ = key;
        Err(crate::Unimplemented::new("Credential::store_in_keychain", "U6").into())
    }
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
/// wire format and their stream back into [`StreamEvent`]s.
pub trait ChatProvider: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    fn stream<'a>(&'a self, req: &'a Request, tx: mpsc::Sender<StreamEvent>, cancel: CancellationToken) -> StreamFuture<'a>;
}

/// The Claude client over HTTPS.
#[derive(Debug, Clone)]
pub struct ClaudeClient {
    pub credential: Credential,
    pub base_url: String,
}

impl ClaudeClient {
    pub fn new(credential: Credential) -> Self {
        Self { credential, base_url: "https://api.anthropic.com".to_string() }
    }
}

impl ChatProvider for ClaudeClient {
    fn id(&self) -> &str {
        "claude"
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { tools: true, thinking: true, effort: true, cache_control: true }
    }
    fn stream<'a>(&'a self, req: &'a Request, tx: mpsc::Sender<StreamEvent>, cancel: CancellationToken) -> StreamFuture<'a> {
        let _ = (req, tx, cancel);
        Box::pin(async { Err(crate::Unimplemented::new("ClaudeClient::stream", "U6").into()) })
    }
}

/// The operator's answer to an approval card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    Once,
    ForSession,
    Deny,
}

#[derive(Debug)]
pub enum AgentEvent {
    Started,
    TextDelta(String),
    ThinkingDelta(String),
    ToolUseStarted { id: String, name: String },
    ToolInputDelta { id: String, partial_json: String },
    /// a write tool waits for the operator; answer through `approval`
    ToolCallPending { id: String, name: String, input: serde_json::Value, approval: oneshot::Sender<Approval> },
    ToolResult { id: String, name: String, outcome: ToolOutcome, millis: u128 },
    TurnDone { stop_reason: StopReason, usage: Usage },
    Refused { category: Option<String>, explanation: Option<String>, recommended_model: Option<String> },
    Failed(ClaudeError),
    Cancelled,
}

/// The tool bridge: MCP tools as Claude tool definitions, and results back.
pub fn tool_defs(tools: &[ToolInfo]) -> Vec<ToolDef> {
    let _ = tools;
    Vec::new()
}

/// The agent over one estate session.
pub struct Agent {
    pub provider: Arc<dyn ChatProvider>,
    pub session: Arc<EstateSession>,
    pub model: String,
    pub effort: Effort,
    pub fallbacks: bool,
    pub auto_approve_writes: bool,
    pub messages: Vec<Message>,
    /// tools the operator allowed for the session
    pub allowed: BTreeMap<String, ()>,
}

impl Agent {
    pub fn new(provider: Arc<dyn ChatProvider>, session: Arc<EstateSession>, model: String, effort: Effort) -> Self {
        Self { provider, session, model, effort, fallbacks: true, auto_approve_writes: false, messages: Vec::new(), allowed: BTreeMap::new() }
    }

    /// One user turn: stream, execute the tool calls with approval, loop until the
    /// model ends the turn. The transcript stays append-only; a cancelled or refused
    /// turn is discarded whole.
    pub async fn run_turn(&mut self, user_text: String, events: mpsc::Sender<AgentEvent>, cancel: CancellationToken) -> Result<(), ClaudeError> {
        let _ = (user_text, events, cancel);
        Err(crate::Unimplemented::new("Agent::run_turn", "U6").into())
    }
}
