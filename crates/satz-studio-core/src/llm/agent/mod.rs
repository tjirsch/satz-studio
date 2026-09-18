//! The agent loop over one estate: the request built from the tool host, the stream
//! folded into a response, every tool call executed under the approval gate, all
//! results returned in one user message, and the loop run until the model ends the
//! turn. [`bridge`] maps MCP tools and outcomes to Claude's blocks.

pub mod bridge;

use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::claude::error::ClaudeError;
use super::claude::types::{
    CacheControl, ContentBlock, Effort, Message, Request, Response, StopReason, SystemBlock, Usage,
};
use super::{ChatProvider, StreamEvent};
use crate::satz::{EstateSession, SatzError, ToolInfo, ToolOutcome};

/// `max_tokens` of every request: streaming, so the model has room.
pub const MAX_TOKENS: u32 = 64_000;
/// How many `pause_turn`s one turn continues through before it is an error.
const MAX_PAUSES: usize = 10;

/// The first block of the system prompt, before the session's instructions and the guide.
pub const PREAMBLE: &str = "You are inside satz-studio, the desktop app for satz. An estate is open beside you and the human sees every tool call you make; \
a call that writes waits for their approval. Edit `.satz` files only through the satz tools — never write a file by hand, and never edit anything under `hcl/`, \
which satz generates. Run `satz_transpile_check` after every write. Never invent an id: an import id, a project number or a directory id comes from `satz_adopt` \
or from the human.";

/// What the agent's tools come from: the estate session in the app, a mock in a test.
pub trait ToolHost: Send + Sync {
    fn tools(&self) -> Vec<ToolInfo>;
    /// The `instructions` the MCP server returned at initialize.
    fn instructions(&self) -> String;
    /// The text of `satz://guide`.
    fn guide(&self) -> String;
    fn call<'a>(
        &'a self,
        name: &'a str,
        args: serde_json::Map<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<ToolOutcome, SatzError>> + Send + 'a>>;
}

impl ToolHost for EstateSession {
    fn tools(&self) -> Vec<ToolInfo> {
        EstateSession::tools(self).to_vec()
    }
    fn instructions(&self) -> String {
        EstateSession::instructions(self).to_string()
    }
    fn guide(&self) -> String {
        EstateSession::guide(self).to_string()
    }
    /// A tool that is not read-only runs under the estate's write lock, like every
    /// other writer.
    fn call<'a>(
        &'a self,
        name: &'a str,
        args: serde_json::Map<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<ToolOutcome, SatzError>> + Send + 'a>> {
        Box::pin(async move {
            let writes = !self
                .tool_info(name)
                .is_some_and(|t| t.annotations.is_read_only());
            let _guard = if writes {
                Some(self.write_lock().await)
            } else {
                None
            };
            self.tool(name, args).await
        })
    }
}

/// The volatile half of the system prompt: what the open estate is right now. The
/// caller fills it ([`Agent::set_context`]); it is rendered as short lines with no
/// cache breakpoint.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EstateContext {
    pub path: String,
    pub runs_as: Option<String>,
    pub deployment_mode: Option<String>,
    pub questions_summary: Option<String>,
    pub diagnostics: Vec<String>,
    /// the outline as indented `type "label"` lines
    pub outline: Vec<String>,
}

impl EstateContext {
    /// How many diagnostics the prompt carries.
    pub const MAX_DIAGNOSTICS: usize = 20;

    pub fn render(&self) -> String {
        let mut lines = vec![format!("estate: {}", self.path)];
        if let Some(runs_as) = &self.runs_as {
            lines.push(format!("runs as: {runs_as}"));
        }
        if let Some(mode) = &self.deployment_mode {
            lines.push(format!("deployment mode: {mode}"));
        }
        if let Some(summary) = &self.questions_summary {
            lines.push(format!("questions: {summary}"));
        }
        if !self.diagnostics.is_empty() {
            lines.push(format!("diagnostics ({}):", self.diagnostics.len()));
            lines.extend(
                self.diagnostics
                    .iter()
                    .take(Self::MAX_DIAGNOSTICS)
                    .map(|d| format!("  {d}")),
            );
            if self.diagnostics.len() > Self::MAX_DIAGNOSTICS {
                lines.push(format!(
                    "  … and {} more",
                    self.diagnostics.len() - Self::MAX_DIAGNOSTICS
                ));
            }
        }
        if !self.outline.is_empty() {
            lines.push("outline:".to_string());
            lines.extend(self.outline.iter().map(|o| format!("  {o}")));
        }
        lines.join("\n")
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
    /// a request's stream has started (once per request of the turn)
    Started,
    TextDelta(String),
    ThinkingDelta(String),
    ToolUseStarted {
        id: String,
        name: String,
    },
    ToolInputDelta {
        id: String,
        partial_json: String,
    },
    /// a write tool waits for the operator; answer through `approval`
    ToolCallPending {
        id: String,
        name: String,
        input: serde_json::Value,
        approval: oneshot::Sender<Approval>,
    },
    ToolResult {
        id: String,
        name: String,
        outcome: ToolOutcome,
        millis: u128,
    },
    /// something the engine says about the turn that is not part of the answer: the
    /// subscription's usage against the plan, a retry, a tool the engine denied
    /// itself. The Chat view shows the last one in its footer.
    Notice(String),
    /// the turn ended; `usage` is the last request's
    TurnDone {
        stop_reason: StopReason,
        usage: Usage,
    },
    Refused {
        category: Option<String>,
        explanation: Option<String>,
        recommended_model: Option<String>,
    },
    /// the turn failed and was discarded; the same error is returned from `run_turn`
    Failed(ClaudeError),
    Cancelled,
}

/// The agent over one tool host.
pub struct Agent {
    pub provider: Arc<dyn ChatProvider>,
    pub host: Arc<dyn ToolHost>,
    pub model: String,
    pub effort: Effort,
    pub fallbacks: bool,
    pub auto_approve_writes: bool,
    /// the transcript: append-only, valid for the next request at every return
    pub messages: Vec<Message>,
    /// tools the operator allowed for the session
    pub allowed: BTreeMap<String, ()>,
    pub context: Option<EstateContext>,
}

impl Agent {
    pub fn new(
        provider: Arc<dyn ChatProvider>,
        session: Arc<EstateSession>,
        model: String,
        effort: Effort,
    ) -> Self {
        Self::with_host(provider, session, model, effort)
    }

    pub fn with_host(
        provider: Arc<dyn ChatProvider>,
        host: Arc<dyn ToolHost>,
        model: String,
        effort: Effort,
    ) -> Self {
        Self {
            provider,
            host,
            model,
            effort,
            fallbacks: true,
            auto_approve_writes: false,
            messages: Vec::new(),
            allowed: BTreeMap::new(),
            context: None,
        }
    }

    pub fn set_context(&mut self, context: EstateContext) {
        self.context = Some(context);
    }

    /// The request the next turn sends: `system[0]` the preamble, the host's
    /// instructions and its guide with the cache breakpoint, `system[1]` the estate
    /// context, the tools from the bridge, and the transcript.
    pub fn request(&self) -> Request {
        let mut guide = vec![PREAMBLE.to_string()];
        for part in [self.host.instructions(), self.host.guide()] {
            let part = part.trim().to_string();
            if !part.is_empty() {
                guide.push(part);
            }
        }
        let mut system = vec![SystemBlock {
            text: guide.join("\n\n"),
            cache_control: Some(CacheControl::ephemeral()),
        }];
        if let Some(context) = &self.context {
            system.push(SystemBlock {
                text: context.render(),
                cache_control: None,
            });
        }
        Request {
            model: self.model.clone(),
            max_tokens: MAX_TOKENS,
            system,
            messages: self.messages.clone(),
            tools: bridge::tool_defs(&self.host.tools()),
            effort: self.effort,
            fallbacks: self.fallbacks,
        }
    }

    /// One user turn: stream, execute the tool calls with approval, loop until the
    /// model ends the turn. A turn that ends in [`AgentEvent::TurnDone`] has appended
    /// its messages; a turn that is refused, cancelled or fails leaves `messages` as
    /// it was before the turn, so the transcript is valid for the next request either
    /// way.
    pub async fn run_turn(
        &mut self,
        user_text: String,
        events: mpsc::Sender<AgentEvent>,
        cancel: CancellationToken,
    ) -> Result<(), ClaudeError> {
        let start = self.messages.len();
        match self.turn(user_text, &events, &cancel).await {
            Ok(()) => Ok(()),
            Err(e) => {
                self.messages.truncate(start);
                let event = match &e {
                    ClaudeError::Cancelled => AgentEvent::Cancelled,
                    ClaudeError::Refused {
                        category,
                        explanation,
                        recommended_model,
                    } => AgentEvent::Refused {
                        category: category.clone(),
                        explanation: explanation.clone(),
                        recommended_model: recommended_model.clone(),
                    },
                    other => AgentEvent::Failed(other.clone()),
                };
                // the receiver may be gone; the error is returned either way
                let _ = events.send(event).await;
                Err(e)
            }
        }
    }

    async fn turn(
        &mut self,
        user_text: String,
        events: &mpsc::Sender<AgentEvent>,
        cancel: &CancellationToken,
    ) -> Result<(), ClaudeError> {
        let mut next_user = Some(Message::user(vec![ContentBlock::text(user_text)]));
        let mut pauses = 0;
        loop {
            if let Some(message) = next_user.take() {
                self.messages.push(message);
            }
            let request = self.request();
            let response = self.stream_once(&request, events, cancel).await?;
            self.messages
                .push(Message::assistant(response.content.clone()));
            match response.stop_reason {
                StopReason::ToolUse => {
                    let results = self.execute_tools(&response, events, cancel).await?;
                    if results.is_empty() {
                        return Err(ClaudeError::Stream(
                            "stop_reason tool_use without a tool_use block".to_string(),
                        ));
                    }
                    next_user = Some(Message::user(results));
                }
                StopReason::EndTurn | StopReason::MaxTokens | StopReason::StopSequence => {
                    send(
                        events,
                        AgentEvent::TurnDone {
                            stop_reason: response.stop_reason,
                            usage: response.usage,
                        },
                    )
                    .await?;
                    return Ok(());
                }
                StopReason::PauseTurn => {
                    pauses += 1;
                    if pauses > MAX_PAUSES {
                        return Err(ClaudeError::Stream(format!(
                            "the model paused {pauses} times in one turn"
                        )));
                    }
                }
                StopReason::Refusal => {
                    let details = response.stop_details.unwrap_or_default();
                    return Err(ClaudeError::Refused {
                        category: details.category,
                        explanation: details.explanation,
                        recommended_model: details.recommended_model,
                    });
                }
                StopReason::Unknown => {
                    return Err(ClaudeError::Stream(
                        "a stop reason this code does not know".to_string(),
                    ));
                }
            }
        }
    }

    /// One request: stream it, forward the deltas, fold the blocks into the response.
    async fn stream_once(
        &self,
        request: &Request,
        events: &mpsc::Sender<AgentEvent>,
        cancel: &CancellationToken,
    ) -> Result<Response, ClaudeError> {
        let (tx, mut rx) = mpsc::channel(64);
        let streaming = self.provider.stream(request, tx, cancel.clone());
        let forwarding = async {
            let mut fold = Fold::default();
            let mut tool_ids: BTreeMap<usize, String> = BTreeMap::new();
            while let Some(event) = rx.recv().await {
                let forwarded = match event {
                    StreamEvent::Started { id, model } => {
                        fold.id = Some(id);
                        fold.model = model;
                        Some(AgentEvent::Started)
                    }
                    StreamEvent::TextDelta(text) => Some(AgentEvent::TextDelta(text)),
                    StreamEvent::ThinkingDelta(text) => Some(AgentEvent::ThinkingDelta(text)),
                    StreamEvent::ToolUseStart { index, id, name } => {
                        tool_ids.insert(index, id.clone());
                        Some(AgentEvent::ToolUseStarted { id, name })
                    }
                    StreamEvent::ToolInputDelta {
                        index,
                        partial_json,
                    } => {
                        let id = tool_ids.get(&index).cloned().ok_or_else(|| {
                            ClaudeError::Stream(format!(
                                "an input delta for block {index}, which is not a tool call"
                            ))
                        })?;
                        Some(AgentEvent::ToolInputDelta { id, partial_json })
                    }
                    StreamEvent::BlockStop { index, block } => {
                        if index != fold.content.len() {
                            return Err(ClaudeError::Stream(format!(
                                "block {index} completed where block {} was expected",
                                fold.content.len()
                            )));
                        }
                        fold.content.push(block);
                        None
                    }
                    StreamEvent::Done {
                        stop_reason,
                        stop_details,
                        usage,
                    } => {
                        fold.done = Some((stop_reason, stop_details, usage));
                        None
                    }
                    // the provider returns the error; nothing to forward
                    StreamEvent::Error(_) => None,
                };
                if let Some(event) = forwarded
                    && events.send(event).await.is_err()
                {
                    cancel.cancel();
                    return Err(ClaudeError::Cancelled);
                }
            }
            Ok(fold)
        };
        let (streamed, folded) = tokio::join!(streaming, forwarding);
        streamed?;
        let fold = folded?;
        if cancel.is_cancelled() {
            return Err(ClaudeError::Cancelled);
        }
        fold.finish()
    }

    /// Every `tool_use` block of the response, in order, under the approval gate.
    async fn execute_tools(
        &mut self,
        response: &Response,
        events: &mpsc::Sender<AgentEvent>,
        cancel: &CancellationToken,
    ) -> Result<Vec<ContentBlock>, ClaudeError> {
        let tools = self.host.tools();
        let mut results = Vec::new();
        for block in &response.content {
            let ContentBlock::ToolUse { id, name, input } = block else {
                continue;
            };
            let info = tools.iter().find(|t| &t.name == name);
            let outcome = match (info, input.as_object()) {
                (None, _) => Outcome::refused(format!("no such tool: {name}")),
                (Some(_), None) => {
                    Outcome::refused("the tool input is not a JSON object".to_string())
                }
                (Some(info), Some(args)) => {
                    if self.runs_without_asking(info) {
                        self.run_tool(name, args.clone(), cancel).await?
                    } else {
                        let (ack, answer) = oneshot::channel();
                        send(
                            events,
                            AgentEvent::ToolCallPending {
                                id: id.clone(),
                                name: name.clone(),
                                input: input.clone(),
                                approval: ack,
                            },
                        )
                        .await?;
                        let approval = tokio::select! {
                            a = answer => a.map_err(|_| ClaudeError::Tool { name: name.clone(), message: "the approval card was dropped without an answer".to_string() })?,
                            _ = cancel.cancelled() => return Err(ClaudeError::Cancelled),
                        };
                        match approval {
                            Approval::Once => self.run_tool(name, args.clone(), cancel).await?,
                            Approval::ForSession => {
                                self.allowed.insert(name.clone(), ());
                                self.run_tool(name, args.clone(), cancel).await?
                            }
                            Approval::Deny => {
                                Outcome::refused("denied by the operator".to_string())
                            }
                        }
                    }
                }
            };
            send(
                events,
                AgentEvent::ToolResult {
                    id: id.clone(),
                    name: name.clone(),
                    outcome: outcome.outcome.clone(),
                    millis: outcome.millis,
                },
            )
            .await?;
            results.push(bridge::tool_result(id, &outcome.outcome));
        }
        Ok(results)
    }

    /// Without a card: a read-only tool; a non-destructive one when writes are
    /// auto-approved or when the operator allowed the tool for the session. A
    /// destructive tool asks every time, whatever was allowed before.
    fn runs_without_asking(&self, tool: &ToolInfo) -> bool {
        let a = &tool.annotations;
        if a.is_read_only() {
            return true;
        }
        let destructive = a.destructive == Some(true);
        !destructive && (self.auto_approve_writes || self.allowed.contains_key(&tool.name))
    }

    /// The call runs to completion on its own task: cancellation stops the wait and
    /// drops the result, never a write half-done. Parameters the server refuses as a
    /// JSON-RPC `invalid_params` error are an error result carrying satz's message: the
    /// model chose the name and the arguments, so it is told and the turn goes on. Any
    /// other failure below the tool fails the turn.
    async fn run_tool(
        &self,
        name: &str,
        args: serde_json::Map<String, serde_json::Value>,
        cancel: &CancellationToken,
    ) -> Result<Outcome, ClaudeError> {
        let host = Arc::clone(&self.host);
        let tool = name.to_string();
        let started = Instant::now();
        let task = tokio::spawn(async move { host.call(&tool, args).await });
        let called = tokio::select! {
            joined = task => joined.map_err(|e| ClaudeError::Tool { name: name.to_string(), message: format!("the tool task ended abnormally: {e}") })?,
            _ = cancel.cancelled() => return Err(ClaudeError::Cancelled),
        };
        let outcome = match called {
            Ok(outcome) => outcome,
            Err(SatzError::InvalidParams { message, .. }) => ToolOutcome {
                structured: None,
                text: message,
                is_error: true,
            },
            Err(e) => {
                return Err(ClaudeError::Tool {
                    name: name.to_string(),
                    message: e.to_string(),
                });
            }
        };
        Ok(Outcome {
            outcome,
            millis: started.elapsed().as_millis(),
        })
    }
}

/// A tool's outcome and how long it took; a refusal made here took no time.
struct Outcome {
    outcome: ToolOutcome,
    millis: u128,
}

impl Outcome {
    fn refused(text: String) -> Self {
        Self {
            outcome: ToolOutcome {
                structured: None,
                text,
                is_error: true,
            },
            millis: 0,
        }
    }
}

/// The response, folded from the stream's complete blocks and its end.
#[derive(Default)]
struct Fold {
    id: Option<String>,
    model: String,
    content: Vec<ContentBlock>,
    done: Option<(StopReason, Option<super::claude::types::StopDetails>, Usage)>,
}

impl Fold {
    fn finish(self) -> Result<Response, ClaudeError> {
        let id = self
            .id
            .ok_or_else(|| ClaudeError::Stream("the stream ended without a start".to_string()))?;
        let (stop_reason, stop_details, usage) = self
            .done
            .ok_or_else(|| ClaudeError::Stream("the stream ended without its end".to_string()))?;
        Ok(Response {
            id,
            model: self.model,
            content: self.content,
            stop_reason,
            stop_details,
            usage,
        })
    }
}

/// A closed event channel means the caller is gone: the turn is cancelled.
async fn send(events: &mpsc::Sender<AgentEvent>, event: AgentEvent) -> Result<(), ClaudeError> {
    events.send(event).await.map_err(|_| ClaudeError::Cancelled)
}
