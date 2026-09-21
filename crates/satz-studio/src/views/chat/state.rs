//! The Chat view's store and the pure parts under it: the reduction of every
//! [`AgentEvent`] into what the list shows, the usage totals, the estate context the
//! agent's system prompt carries, and the replay of a saved transcript into turns.
//! Nothing here touches the agent or the disk; the coroutine (`actions.rs`) does.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use satz_studio_core::diag::{Diagnostic, Severity};
use satz_studio_core::llm::{
    AgentEvent, Approval, Capabilities, ContentBlock, Effort, EstateContext, Message, Role,
    StopReason, Usage, result_text,
};
use satz_studio_core::model::{EstateModel, ResourceKind, ResourceNode};
use satz_studio_core::satz::reports::{QuestionsReport, QuestionsSummary};
use tokio::sync::oneshot;

use super::summary::summary_line;

/// Whether the agent behind the view exists, and why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    /// the provider is being resolved
    Starting,
    Ready,
    /// no Claude credential; `tried` is what each of the four sources answered
    NoCredential {
        tried: Vec<String>,
    },
    /// the Claude Code engine, with a CLI that is signed out; `login` is the shell
    /// line the Sign in button runs in the user's terminal
    NotSignedIn {
        login: String,
    },
    Failed(String),
}

/// Which engine serves the chat: the app's own loop over the Messages API, or the
/// installed Claude Code CLI on the user's subscription (ADR 0010). The rail, the
/// composer and the footer each show something different for the two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Api,
    ClaudeCode,
}

impl EngineKind {
    pub fn is_claude_code(self) -> bool {
        matches!(self, EngineKind::ClaudeCode)
    }
}

/// One turn of the conversation as the list shows it. Only these two mirror the
/// agent's transcript; a refused, failed or cancelled turn becomes a [`Notice`].
#[derive(Debug, Clone, PartialEq)]
pub enum TurnView {
    User { text: String },
    Assistant(AssistantTurn),
}

/// The model's side of one turn: its blocks in stream order — text, thinking, one
/// card per tool call — across every request the turn took.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AssistantTurn {
    pub blocks: Vec<Block>,
    /// how many requests the turn took: one, plus one per tool loop
    pub requests: usize,
    /// the model the server named at the start of the turn's latest request; `None` for
    /// a replayed turn, whose transcript does not keep it
    pub model: Option<String>,
    pub stop_reason: Option<StopReason>,
    /// the turn's usage: the requests that ended so far while it streams, the engine's
    /// total for the turn once it is done; `None` for a replayed turn, which has none
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Text(String),
    /// the summarised thinking
    Thinking(String),
    /// a thinking block the API redacted
    RedactedThinking,
    Tool(ToolCard),
    /// the request was re-run on another model (a `fallback` block in the transcript)
    Fallback {
        from: String,
        to: String,
    },
    /// a block kind this view does not read, carried by the transcript verbatim
    Other(String),
}

/// One tool call as the conversation shows it: its name, its status, its duration and
/// one line about its result. The input and the result themselves are the debug log's
/// ([`DebugEvent`], under the same id).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCard {
    pub id: String,
    pub name: String,
    /// the input JSON as it streamed, partial until the block stops; kept for the debug
    /// log, never shown on the card
    pub input_json: String,
    /// `None` while the call runs
    pub result: Option<ToolResultView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolResultView {
    /// the one line [`summary_line`] makes of what the model read
    pub summary: String,
    pub is_error: bool,
    /// `None` for a replayed result, whose duration was not kept
    pub millis: Option<u128>,
}

impl ToolCard {
    fn open(id: String, name: String) -> Self {
        Self {
            id,
            name,
            input_json: String::new(),
            result: None,
        }
    }

    /// What the card renders, and nothing else.
    pub fn shown(&self) -> CardText {
        CardText {
            name: self.name.clone(),
            summary: self.result.as_ref().map(|r| r.summary.clone()),
            duration: self
                .result
                .as_ref()
                .and_then(|r| r.millis)
                .map(|ms| format!("{ms} ms")),
        }
    }

    /// Every text the card renders.
    #[cfg(test)]
    pub fn texts(&self) -> Vec<String> {
        let shown = self.shown();
        let mut texts = vec![shown.name];
        texts.extend(shown.summary);
        texts.extend(shown.duration);
        texts
    }

    /// The input the stream carried, as the debug log keeps it: pretty-printed when it
    /// parses, `{}` when nothing streamed, the raw text when it does not parse.
    fn streamed_input(&self) -> String {
        let raw = self.input_json.trim();
        if raw.is_empty() {
            return "{}".to_string();
        }
        match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(value) => pretty(&value),
            Err(_) => self.input_json.clone(),
        }
    }
}

/// The texts of a tool card: the name, the summary line once the call is done, and
/// `42 ms` once its duration is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardText {
    pub name: String,
    pub summary: Option<String>,
    pub duration: Option<String>,
}

/// What the debug log holds of the satz stderr lines of a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StderrLines {
    /// the lines the estate's `satz mcp` wrote while the call was open
    Lines(Vec<String>),
    /// this call's stderr does not reach the app, and why
    Unseen(&'static str),
}

/// Why a Claude Code call has no stderr in the log.
pub const CLAUDE_CODE_STDERR: &str =
    "Claude Code runs its own satz mcp; that process's stderr does not reach the app";
/// Why a replayed call has no stderr in the log.
pub const REPLAYED_STDERR: &str = "a transcript does not keep satz's stderr";

/// One tool call in the debug log, under its call id: the input and the result JSON,
/// whether it was an error, how long it took, and what satz wrote to stderr meanwhile.
#[derive(Debug, Clone, PartialEq)]
pub struct DebugEvent {
    pub id: String,
    pub name: String,
    /// the whole input, pretty-printed; `None` until it is known
    pub input: Option<String>,
    /// `None` while the call runs
    pub result: Option<DebugResult>,
    pub stderr: StderrLines,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugResult {
    /// what the model read: the text `result_text` makes of the outcome, which a
    /// replayed transcript carries as it was sent
    pub body: String,
    pub is_error: bool,
    /// `None` for a replayed result, whose duration was not kept
    pub millis: Option<u128>,
}

impl DebugEvent {
    fn open(id: String, name: String, stderr: StderrLines) -> Self {
        Self {
            id,
            name,
            input: None,
            result: None,
            stderr,
        }
    }
}

impl AssistantTurn {
    fn push_text(&mut self, piece: &str) {
        match self.blocks.last_mut() {
            Some(Block::Text(text)) => text.push_str(piece),
            _ => self.blocks.push(Block::Text(piece.to_string())),
        }
    }

    fn push_thinking(&mut self, piece: &str) {
        match self.blocks.last_mut() {
            Some(Block::Thinking(text)) => text.push_str(piece),
            _ => self.blocks.push(Block::Thinking(piece.to_string())),
        }
    }

    fn card_mut(&mut self, id: &str) -> Option<&mut ToolCard> {
        self.blocks.iter_mut().rev().find_map(|b| match b {
            Block::Tool(card) if card.id == id => Some(card),
            _ => None,
        })
    }
}

/// A write tool waiting for the operator; the sender that answers it is parked in
/// the coroutine, since the store cannot hold it.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// How a turn ended without reaching the transcript. The message that began it is
/// kept so it can be sent again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    Refused {
        user_text: String,
        category: Option<String>,
        explanation: Option<String>,
        recommended_model: Option<String>,
    },
    Failed {
        user_text: String,
        error: String,
    },
    Cancelled {
        user_text: String,
    },
}

impl Notice {
    pub fn user_text(&self) -> &str {
        match self {
            Notice::Refused { user_text, .. }
            | Notice::Failed { user_text, .. }
            | Notice::Cancelled { user_text } => user_text,
        }
    }
}

/// The usage figures the footer shows: the last turn's and the session's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UsageTotals {
    /// the last completed turn, every request of it
    pub turn: Usage,
    /// every completed turn summed
    pub session: Usage,
    pub turns: usize,
}

impl UsageTotals {
    pub fn add(&mut self, usage: Usage) {
        self.turn = usage;
        self.turns += 1;
        self.session = self.session.plus(usage);
    }
}

#[derive(Store)]
pub struct ChatStore {
    pub agent: AgentStatus,
    /// the completed turns, mirroring the agent's transcript
    pub turns: Vec<TurnView>,
    /// every tool call of the conversation, in order, with its JSON: what the debug
    /// panel shows, kept whether the panel is on or not
    pub debug: Vec<DebugEvent>,
    /// the call a tool card's link asked the debug panel to show
    pub debug_focus: Option<String>,
    /// where the running turn's calls begin in `debug`: a call before it that has no
    /// result belongs to a turn that ended first, and no stderr line is its
    pub debug_turn: usize,
    /// the assistant turn being streamed
    pub streaming: Option<AssistantTurn>,
    pub pending: Option<PendingCall>,
    /// how the last turn ended when it did not reach the transcript
    pub notice: Option<Notice>,
    pub usage: UsageTotals,
    /// the transcript file the conversation is appended to
    pub transcript: Option<PathBuf>,
    /// this estate's transcripts, newest first
    pub transcripts: Vec<PathBuf>,
    pub model: String,
    pub effort: Effort,
    pub engine: EngineKind,
    pub capabilities: Capabilities,
    /// the last thing the engine said about the turn that is not the answer: the
    /// subscription's usage against the plan, a retry, a tool the engine denied itself
    pub engine_notice: Option<String>,
    /// a turn is running: Send is off, Cancel is on
    pub busy: bool,
    /// something outside a turn went wrong: a transcript not written, an action refused
    pub error: Option<String>,
}

impl ChatStore {
    pub fn new(model: String, effort: Effort, engine: EngineKind) -> Self {
        Self {
            agent: AgentStatus::Starting,
            turns: Vec::new(),
            debug: Vec::new(),
            debug_focus: None,
            debug_turn: 0,
            streaming: None,
            pending: None,
            notice: None,
            usage: UsageTotals::default(),
            transcript: None,
            transcripts: Vec::new(),
            model,
            effort,
            engine,
            capabilities: Capabilities {
                tools: true,
                thinking: true,
                effort: true,
                cache_control: true,
            },
            engine_notice: None,
            busy: false,
            error: None,
        }
    }

    /// The operator sent a message: it shows at once, and the assistant turn opens.
    pub fn begin_turn(&mut self, text: String) {
        self.notice = None;
        self.error = None;
        self.turns.push(TurnView::User { text });
        self.debug_turn = self.debug.len();
        self.streaming = Some(AssistantTurn::default());
        self.busy = true;
    }

    /// Reduce one event. A `ToolCallPending` hands its sender back: the store cannot
    /// hold it, and the coroutine parks it until the operator answers.
    pub fn apply(&mut self, event: AgentEvent) -> Option<oneshot::Sender<Approval>> {
        match event {
            AgentEvent::Started { model } => {
                // Claude Code names its model when the turn starts, not at spawn: the
                // footer follows it from here
                if self.engine.is_claude_code() {
                    self.model = model.clone();
                }
                self.with_streaming("Started", |turn| {
                    turn.requests += 1;
                    turn.model = Some(model);
                });
            }
            AgentEvent::RequestDone { usage } => self.with_streaming("RequestDone", |turn| {
                turn.usage = Some(turn.usage.unwrap_or_default().plus(usage));
            }),
            AgentEvent::TextDelta(_)
            | AgentEvent::ThinkingDelta(_)
            | AgentEvent::ToolInputDelta { .. } => {
                if let Err(e) = apply_delta(&mut self.streaming, event) {
                    self.error = Some(e);
                }
            }
            AgentEvent::ToolUseStarted { id, name } => {
                let stderr = self.fresh_stderr();
                let entry = DebugEvent::open(id.clone(), name.clone(), stderr);
                self.with_streaming("ToolUseStarted", |turn| {
                    turn.blocks.push(Block::Tool(ToolCard::open(id, name)));
                });
                self.debug.push(entry);
            }
            AgentEvent::ToolCallPending {
                id,
                name,
                input,
                approval,
            } => {
                self.debug_entry_mut(&id, &name).input = Some(pretty(&input));
                self.pending = Some(PendingCall { id, name, input });
                return Some(approval);
            }
            AgentEvent::ToolResult {
                id,
                name,
                outcome,
                millis,
            } => {
                let body = result_text(&outcome);
                let mut streamed = None;
                self.with_streaming("ToolResult", |turn| {
                    if let Some(card) = turn.card_mut(&id) {
                        card.result = Some(ToolResultView {
                            summary: summary_line(&body, outcome.is_error),
                            is_error: outcome.is_error,
                            millis: Some(millis),
                        });
                        streamed = Some(card.streamed_input());
                    }
                });
                if streamed.is_none() && self.error.is_none() {
                    self.error = Some(format!(
                        "a result for {name} ({id}) arrived without its card"
                    ));
                }
                let entry = self.debug_entry_mut(&id, &name);
                if entry.input.is_none() {
                    entry.input = streamed;
                }
                entry.result = Some(DebugResult {
                    body,
                    is_error: outcome.is_error,
                    millis: Some(millis),
                });
                self.pending = None;
            }
            AgentEvent::Notice(text) => self.engine_notice = Some(text),
            AgentEvent::TurnDone { stop_reason, usage } => {
                match self.streaming.take() {
                    Some(mut turn) => {
                        turn.stop_reason = Some(stop_reason);
                        turn.usage = Some(usage);
                        self.turns.push(TurnView::Assistant(turn));
                        self.usage.add(usage);
                    }
                    None => self.error = Some("TurnDone arrived outside a turn".to_string()),
                }
                self.pending = None;
                self.busy = false;
            }
            AgentEvent::Refused {
                category,
                explanation,
                recommended_model,
            } => self.end_discarded(|user_text| Notice::Refused {
                user_text,
                category,
                explanation,
                recommended_model,
            }),
            AgentEvent::Failed(e) => self.end_discarded(|user_text| Notice::Failed {
                user_text,
                error: e.to_string(),
            }),
            AgentEvent::Cancelled => {
                self.end_discarded(|user_text| Notice::Cancelled { user_text })
            }
        }
        None
    }

    /// The event channel closed. A turn that is still open ended without its
    /// terminal event, which the agent never does; it is closed here as a failure so
    /// the view never stays busy.
    pub fn turn_closed(&mut self) {
        if self.busy {
            self.end_discarded(|user_text| Notice::Failed {
                user_text,
                error: "the turn ended without a result".to_string(),
            });
        }
    }

    /// A line the estate's `satz mcp` wrote to stderr. It belongs to the running turn's
    /// first call still without its result — the API engine runs a response's calls one
    /// after another, in order — and to nothing when no call is open. On the Claude Code
    /// engine the estate's server does not run the chat's calls, so no line is theirs.
    pub fn debug_stderr(&mut self, line: String) {
        if !self.has_open_call() {
            return;
        }
        let open = self.debug[self.debug_turn..]
            .iter_mut()
            .find(|e| e.result.is_none());
        if let Some(DebugEvent {
            stderr: StderrLines::Lines(lines),
            ..
        }) = open
        {
            lines.push(line);
        }
    }

    /// Whether a call is open for a stderr line to belong to.
    pub fn has_open_call(&self) -> bool {
        self.busy
            && !self.engine.is_claude_code()
            && self
                .debug
                .get(self.debug_turn..)
                .is_some_and(|calls| calls.iter().any(|e| e.result.is_none()))
    }

    /// The debug log's entry for a call id.
    #[cfg(test)]
    pub fn debug_entry(&self, id: &str) -> Option<&DebugEvent> {
        self.debug.iter().find(|e| e.id == id)
    }

    /// The entry for `id`, opened here when the stream never announced the call — a
    /// Claude Code approval for a call its stream did not carry.
    fn debug_entry_mut(&mut self, id: &str, name: &str) -> &mut DebugEvent {
        match self.debug.iter().position(|e| e.id == id) {
            Some(i) => &mut self.debug[i],
            None => {
                let stderr = self.fresh_stderr();
                self.debug
                    .push(DebugEvent::open(id.to_string(), name.to_string(), stderr));
                self.debug.last_mut().expect("just pushed")
            }
        }
    }

    fn fresh_stderr(&self) -> StderrLines {
        if self.engine.is_claude_code() {
            StderrLines::Unseen(CLAUDE_CODE_STDERR)
        } else {
            StderrLines::Lines(Vec::new())
        }
    }

    /// A new conversation on `model`: everything of the old one goes.
    pub fn reset_conversation(&mut self, model: String) {
        self.turns.clear();
        self.debug.clear();
        self.debug_focus = None;
        self.debug_turn = 0;
        self.streaming = None;
        self.pending = None;
        self.notice = None;
        self.usage = UsageTotals::default();
        self.transcript = None;
        self.model = model;
        self.error = None;
    }

    /// A transcript resumed: its turns and its calls' debug log, its file, its model.
    pub fn load_conversation(&mut self, replayed: Replayed, path: PathBuf, model: String) {
        self.reset_conversation(model);
        self.turns = replayed.turns;
        self.debug = replayed.debug;
        self.transcript = Some(path);
    }

    fn with_streaming(&mut self, what: &str, f: impl FnOnce(&mut AssistantTurn)) {
        match &mut self.streaming {
            Some(turn) => f(turn),
            None => self.error = Some(format!("{what} arrived outside a turn")),
        }
    }

    /// The turn did not reach the transcript: the partial answer goes, and the message
    /// that began it comes back out of `turns` into the notice.
    fn end_discarded(&mut self, notice: impl FnOnce(String) -> Notice) {
        self.streaming = None;
        self.pending = None;
        self.busy = false;
        match self.turns.pop() {
            Some(TurnView::User { text }) => self.notice = Some(notice(text)),
            Some(other) => {
                self.turns.push(other);
                self.error = Some(
                    "the turn ended, but the last turn is not the message it began with"
                        .to_string(),
                );
            }
            None => self.error = Some("the turn ended before it began".to_string()),
        }
    }
}

/// The events that arrive many times a second; the coroutine writes them to the
/// `streaming` field alone so the rest of the view stays still.
pub fn is_delta(event: &AgentEvent) -> bool {
    matches!(
        event,
        AgentEvent::TextDelta(_) | AgentEvent::ThinkingDelta(_) | AgentEvent::ToolInputDelta { .. }
    )
}

/// Append a delta to the streaming turn.
pub fn apply_delta(streaming: &mut Option<AssistantTurn>, event: AgentEvent) -> Result<(), String> {
    let Some(turn) = streaming else {
        return Err(format!("{} arrived outside a turn", event_name(&event)));
    };
    match event {
        AgentEvent::TextDelta(piece) => turn.push_text(&piece),
        AgentEvent::ThinkingDelta(piece) => turn.push_thinking(&piece),
        AgentEvent::ToolInputDelta { id, partial_json } => match turn.card_mut(&id) {
            Some(card) => card.input_json.push_str(&partial_json),
            None => return Err(format!("an input delta for {id} arrived without its card")),
        },
        other => return Err(format!("{} is not a delta", event_name(&other))),
    }
    Ok(())
}

fn event_name(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::Started { .. } => "Started",
        AgentEvent::RequestDone { .. } => "RequestDone",
        AgentEvent::TextDelta(_) => "TextDelta",
        AgentEvent::ThinkingDelta(_) => "ThinkingDelta",
        AgentEvent::ToolUseStarted { .. } => "ToolUseStarted",
        AgentEvent::ToolInputDelta { .. } => "ToolInputDelta",
        AgentEvent::ToolCallPending { .. } => "ToolCallPending",
        AgentEvent::ToolResult { .. } => "ToolResult",
        AgentEvent::Notice(_) => "Notice",
        AgentEvent::TurnDone { .. } => "TurnDone",
        AgentEvent::Refused { .. } => "Refused",
        AgentEvent::Failed(_) => "Failed",
        AgentEvent::Cancelled => "Cancelled",
    }
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).expect("a JSON value serialises")
}

/// What the estate context is rendered from.
pub struct ContextInput<'a> {
    pub main: &'a Path,
    /// the estate directory, which diagnostics are shown relative to
    pub dir: &'a Path,
    pub runs_as: Option<&'a str>,
    pub deployment_mode: Option<&'a str>,
    pub model: Option<&'a EstateModel>,
    pub questions: Option<&'a QuestionsReport>,
    pub diagnostics: &'a [Diagnostic],
}

/// The volatile half of the system prompt, from what the estate store holds.
/// [`EstateContext::render`] caps the diagnostics itself.
pub fn estate_context(input: &ContextInput<'_>) -> EstateContext {
    EstateContext {
        path: input.main.display().to_string(),
        runs_as: input.runs_as.map(str::to_string),
        deployment_mode: input.deployment_mode.map(str::to_string),
        questions_summary: input.questions.map(|q| questions_line(&q.summary)),
        diagnostics: input
            .diagnostics
            .iter()
            .map(|d| diagnostic_line(d, input.dir))
            .collect(),
        outline: input
            .model
            .map(|m| {
                let mut lines = Vec::new();
                outline_lines(&m.outline, 0, &mut lines);
                lines
            })
            .unwrap_or_default(),
    }
}

fn questions_line(s: &QuestionsSummary) -> String {
    format!(
        "{} of {} answered, {} unanswered ({} blocking), {} not applicable; {}",
        s.answered,
        s.total,
        s.unanswered,
        s.blocking,
        s.not_applicable,
        if s.complete { "complete" } else { "incomplete" }
    )
}

/// `severity: file:line: message` — the first line of the message, the file relative
/// to the estate directory.
fn diagnostic_line(d: &Diagnostic, base: &Path) -> String {
    let severity = match d.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    };
    let message = d.message.lines().next().unwrap_or_default();
    let file = d
        .file
        .as_deref()
        .map(|f| f.strip_prefix(base).unwrap_or(f).display().to_string());
    match (file, d.line) {
        (Some(f), Some(l)) => format!("{severity}: {f}:{l}: {message}"),
        (Some(f), None) => format!("{severity}: {f}: {message}"),
        (None, Some(l)) => format!("{severity}: line {l}: {message}"),
        (None, None) => format!("{severity}: {message}"),
    }
}

/// The outline as `type "label"` lines, nested resources indented under their
/// parent. Configuration blocks are named and not descended into; nested blocks and
/// member grants are attributes of their resource and stay out.
fn outline_lines(nodes: &[ResourceNode], depth: usize, out: &mut Vec<String>) {
    for node in nodes {
        let line = match node.kind {
            ResourceKind::Config => {
                out.push(format!("{}{}", "  ".repeat(depth), node.key));
                continue;
            }
            ResourceKind::ResourceMap => node.tf_type.clone().unwrap_or_else(|| node.key.clone()),
            ResourceKind::Resource => format!(
                "{} \"{}\"",
                node.tf_type.as_deref().unwrap_or(&node.key),
                node.name().unwrap_or(&node.key)
            ),
            ResourceKind::NestedBlock | ResourceKind::MemberGrant => continue,
            ResourceKind::Unknown => match &node.label {
                Some(label) => format!("{} \"{label}\"", node.key),
                None => node.key.clone(),
            },
        };
        out.push(format!("{}{line}", "  ".repeat(depth)));
        outline_lines(&node.children, depth + 1, out);
    }
}

/// A saved transcript as the view shows it: the turns, and the debug log of every call
/// in them.
#[derive(Debug, Clone, PartialEq)]
pub struct Replayed {
    pub turns: Vec<TurnView>,
    pub debug: Vec<DebugEvent>,
}

/// A saved transcript as turns and their calls' debug log. A user message with text
/// opens a user turn; one carrying only tool results answers the cards of the assistant
/// turn before it; the assistant messages between two user texts fold into one
/// assistant turn, as the stream showed them. Each call's input and result go to the
/// debug log under its id, as a live turn puts them there. A user block this view
/// cannot show is an error naming the message.
pub fn replay(messages: &[Message]) -> Result<Replayed, String> {
    let mut turns = Vec::new();
    let mut debug: Vec<DebugEvent> = Vec::new();
    let mut open: Option<AssistantTurn> = None;
    for (i, message) in messages.iter().enumerate() {
        match message.role {
            Role::User => {
                let mut text = String::new();
                for block in &message.content {
                    match block {
                        ContentBlock::Text { text: piece, .. } => {
                            if !text.is_empty() {
                                text.push_str("\n\n");
                            }
                            text.push_str(piece);
                        }
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                            ..
                        } => {
                            let card = open
                                .as_mut()
                                .and_then(|turn| turn.card_mut(tool_use_id))
                                .ok_or_else(|| {
                                    format!(
                                        "message {}: a result for {tool_use_id} without its call",
                                        i + 1
                                    )
                                })?;
                            card.result = Some(ToolResultView {
                                summary: summary_line(content, *is_error),
                                is_error: *is_error,
                                millis: None,
                            });
                            let entry = debug
                                .iter_mut()
                                .rev()
                                .find(|e| &e.id == tool_use_id)
                                .expect("every card replayed has its debug entry");
                            entry.result = Some(DebugResult {
                                body: content.clone(),
                                is_error: *is_error,
                                millis: None,
                            });
                        }
                        other => {
                            return Err(format!(
                                "message {}: a {} block in a user message",
                                i + 1,
                                other.kind()
                            ));
                        }
                    }
                }
                if !text.is_empty() {
                    if let Some(turn) = open.take() {
                        turns.push(TurnView::Assistant(turn));
                    }
                    turns.push(TurnView::User { text });
                }
            }
            Role::Assistant => {
                let turn = open.get_or_insert_default();
                turn.requests += 1;
                for block in &message.content {
                    match block {
                        ContentBlock::Text { text, .. } => turn.push_text(text),
                        ContentBlock::Thinking { thinking, .. } => {
                            turn.blocks.push(Block::Thinking(thinking.clone()));
                        }
                        ContentBlock::RedactedThinking { .. } => {
                            turn.blocks.push(Block::RedactedThinking);
                        }
                        ContentBlock::ToolUse { id, name, input } => {
                            turn.blocks
                                .push(Block::Tool(ToolCard::open(id.clone(), name.clone())));
                            let mut entry = DebugEvent::open(
                                id.clone(),
                                name.clone(),
                                StderrLines::Unseen(REPLAYED_STDERR),
                            );
                            entry.input = Some(pretty(input));
                            debug.push(entry);
                        }
                        ContentBlock::ToolResult { .. } => {
                            return Err(format!(
                                "message {}: a tool result in an assistant message",
                                i + 1
                            ));
                        }
                        ContentBlock::Other(value)
                            if value.get("type").and_then(serde_json::Value::as_str)
                                == Some("fallback") =>
                        {
                            let field = |k: &str| {
                                value
                                    .get(k)
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("?")
                                    .to_string()
                            };
                            turn.blocks.push(Block::Fallback {
                                from: field("from"),
                                to: field("to"),
                            });
                        }
                        other => turn.blocks.push(Block::Other(other.kind().to_string())),
                    }
                }
            }
        }
    }
    if let Some(turn) = open.take() {
        turns.push(TurnView::Assistant(turn));
    }
    Ok(Replayed { turns, debug })
}

#[cfg(test)]
mod tests {
    use super::*;
    use satz_studio_core::diag::DiagSource;
    use satz_studio_core::llm::ClaudeError;
    use satz_studio_core::satz::ToolOutcome;

    const QUESTIONS: &str =
        include_str!("../../../../satz-studio-core/tests/fixtures/questions-smoke.json");

    fn store() -> ChatStore {
        ChatStore::new("claude-opus-5".to_string(), Effort::High, EngineKind::Api)
    }

    fn started(store: &mut ChatStore, text: &str) {
        store.begin_turn(text.to_string());
        store.apply(AgentEvent::Started {
            model: "claude-opus-5".into(),
        });
    }

    fn usage(input: u64, output: u64, cache_read: Option<u64>) -> Usage {
        Usage {
            input_tokens: input,
            output_tokens: output,
            cache_creation_input_tokens: Some(0),
            cache_read_input_tokens: cache_read,
        }
    }

    #[test]
    fn text_deltas_append_to_the_open_text_block_and_a_tool_call_opens_a_new_one() {
        let mut s = store();
        started(&mut s, "hello");
        s.apply(AgentEvent::TextDelta("Hel".into()));
        s.apply(AgentEvent::TextDelta("lo".into()));
        s.apply(AgentEvent::ThinkingDelta("hm".into()));
        s.apply(AgentEvent::ThinkingDelta("m".into()));
        s.apply(AgentEvent::ToolUseStarted {
            id: "t1".into(),
            name: "satz_questions".into(),
        });
        s.apply(AgentEvent::TextDelta("after".into()));
        let turn = s.streaming.as_ref().unwrap();
        assert_eq!(turn.requests, 1);
        assert_eq!(turn.blocks.len(), 4);
        assert_eq!(turn.blocks[0], Block::Text("Hello".into()));
        assert_eq!(turn.blocks[1], Block::Thinking("hmm".into()));
        assert!(matches!(&turn.blocks[2], Block::Tool(c) if c.name == "satz_questions"));
        assert_eq!(turn.blocks[3], Block::Text("after".into()));
        assert!(s.busy);
        assert_eq!(
            s.turns,
            vec![TurnView::User {
                text: "hello".into()
            }]
        );
        assert_eq!(s.error, None);
    }

    fn card(s: &ChatStore, i: usize) -> ToolCard {
        match &s.streaming.as_ref().unwrap().blocks[i] {
            Block::Tool(c) => c.clone(),
            other => panic!("not a tool card: {other:?}"),
        }
    }

    fn outcome(structured: Option<serde_json::Value>, text: &str, is_error: bool) -> ToolOutcome {
        ToolOutcome {
            structured,
            text: text.into(),
            is_error,
        }
    }

    #[test]
    fn a_tool_card_carries_one_line_and_the_debug_log_carries_the_json() {
        let mut s = store();
        started(&mut s, "check");
        s.apply(AgentEvent::ToolUseStarted {
            id: "t1".into(),
            name: "satz_transpile_check".into(),
        });
        s.apply(AgentEvent::ToolInputDelta {
            id: "t1".into(),
            partial_json: "{\"estate\": ".into(),
        });
        s.apply(AgentEvent::ToolInputDelta {
            id: "t1".into(),
            partial_json: "\"C0example.satz\"}".into(),
        });
        assert!(card(&s, 0).result.is_none());
        let open = s.debug_entry("t1").unwrap();
        assert_eq!(open.input, None, "the input is logged once it is whole");
        assert_eq!(open.result, None);

        s.apply(AgentEvent::ToolResult {
            id: "t1".into(),
            name: "satz_transpile_check".into(),
            outcome: outcome(
                Some(serde_json::json!({"addresses": ["google_folder.infra"]})),
                "ok",
                false,
            ),
            millis: 42,
        });
        let closed = card(&s, 0);
        let result = closed.result.as_ref().unwrap();
        assert!(!result.is_error);
        assert_eq!(result.summary, "addresses: 1");
        assert_eq!(closed.shown().duration.as_deref(), Some("42 ms"));

        let entry = s.debug_entry("t1").unwrap();
        assert_eq!(entry.name, "satz_transpile_check");
        let input: serde_json::Value =
            serde_json::from_str(entry.input.as_deref().unwrap()).unwrap();
        assert_eq!(input, serde_json::json!({"estate": "C0example.satz"}));
        let logged = entry.result.as_ref().unwrap();
        assert!(logged.body.contains("google_folder.infra"));
        assert!(!logged.is_error);
        assert_eq!(logged.millis, Some(42));
        assert_eq!(s.error, None);
    }

    #[test]
    fn a_call_with_no_input_logs_the_empty_object_and_its_card_shows_none() {
        let mut s = store();
        started(&mut s, "who");
        s.apply(AgentEvent::ToolUseStarted {
            id: "t1".into(),
            name: "satz_whoami".into(),
        });
        s.apply(AgentEvent::ToolResult {
            id: "t1".into(),
            name: "satz_whoami".into(),
            outcome: outcome(None, "refused", true),
            millis: 1,
        });
        let card = card(&s, 0);
        assert!(card.result.as_ref().unwrap().is_error);
        assert_eq!(card.result.as_ref().unwrap().summary, "refused");
        assert!(card.texts().iter().all(|t| !t.contains("{}")));
        let entry = s.debug_entry("t1").unwrap();
        assert_eq!(entry.input.as_deref(), Some("{}"));
        assert_eq!(entry.result.as_ref().unwrap().body, "refused");
    }

    #[test]
    fn a_refused_call_shows_its_sentence_and_the_log_keeps_what_it_carries() {
        let mut s = store();
        started(&mut s, "check");
        s.apply(AgentEvent::ToolUseStarted {
            id: "t1".into(),
            name: "satz_transpile_check".into(),
        });
        s.apply(AgentEvent::ToolResult {
            id: "t1".into(),
            name: "satz_transpile_check".into(),
            outcome: outcome(
                Some(serde_json::json!({"findings": [{"kind": "parse"}]})),
                "the estate does not compile",
                true,
            ),
            millis: 3,
        });
        let summary = card(&s, 0).result.unwrap().summary;
        assert_eq!(summary, "the estate does not compile");
        let body = &s.debug_entry("t1").unwrap().result.as_ref().unwrap().body;
        assert!(
            body.starts_with("the estate does not compile\n\n{"),
            "{body}"
        );
        assert!(body.contains("\"kind\": \"parse\""), "{body}");
    }

    #[test]
    fn a_pending_call_parks_its_sender_and_logs_the_input() {
        let mut s = store();
        started(&mut s, "write");
        s.apply(AgentEvent::ToolUseStarted {
            id: "t1".into(),
            name: "satz_interview".into(),
        });
        let (tx, rx) = oneshot::channel();
        let parked = s.apply(AgentEvent::ToolCallPending {
            id: "t1".into(),
            name: "satz_interview".into(),
            input: serde_json::json!({"answers": {"x": true}}),
            approval: tx,
        });
        let parked = parked.expect("the sender is handed back");
        assert_eq!(
            s.pending,
            Some(PendingCall {
                id: "t1".into(),
                name: "satz_interview".into(),
                input: serde_json::json!({"answers": {"x": true}}),
            })
        );
        let logged = s.debug_entry("t1").unwrap().input.clone().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&logged).unwrap(),
            serde_json::json!({"answers": {"x": true}})
        );
        parked.send(Approval::Once).unwrap();
        assert_eq!(rx.blocking_recv().unwrap(), Approval::Once);
    }

    #[test]
    fn a_questions_turn_puts_no_json_on_any_card_and_the_whole_call_in_the_log() {
        let questions: serde_json::Value = serde_json::from_str(QUESTIONS).unwrap();
        let mut s = store();
        started(&mut s, "which questions are open?");
        s.apply(AgentEvent::ToolUseStarted {
            id: "toolu_q".into(),
            name: "satz_questions".into(),
        });
        s.apply(AgentEvent::ToolInputDelta {
            id: "toolu_q".into(),
            partial_json: String::new(),
        });
        s.apply(AgentEvent::ToolResult {
            id: "toolu_q".into(),
            name: "satz_questions".into(),
            outcome: outcome(Some(questions.clone()), "", false),
            millis: 120,
        });
        s.apply(AgentEvent::TextDelta("Some are open.".into()));
        s.apply(AgentEvent::TurnDone {
            stop_reason: StopReason::EndTurn,
            usage: usage(1, 1, None),
        });
        let TurnView::Assistant(turn) = &s.turns[1] else {
            panic!("the assistant turn");
        };
        let cards: Vec<&ToolCard> = turn
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Tool(c) => Some(c),
                _ => None,
            })
            .collect();
        assert_eq!(cards.len(), 1);
        for text in cards.iter().flat_map(|c| c.texts()) {
            assert!(
                !text.contains('{') && !text.contains('['),
                "a card renders JSON: {text}"
            );
        }
        let count = questions["questions"].as_array().unwrap().len();
        assert_eq!(
            cards[0].result.as_ref().unwrap().summary,
            format!("questions: {count}")
        );

        let entry = s
            .debug_entry("toolu_q")
            .expect("the call is logged under its id");
        assert_eq!(entry.input.as_deref(), Some("{}"));
        let logged: serde_json::Value =
            serde_json::from_str(&entry.result.as_ref().unwrap().body).unwrap();
        assert_eq!(logged, questions);
        assert_eq!(entry.result.as_ref().unwrap().millis, Some(120));
    }

    #[test]
    fn stderr_lines_belong_to_the_open_call_of_the_running_turn_only() {
        let mut s = store();
        s.debug_stderr("before any turn".into());
        started(&mut s, "one");
        s.apply(AgentEvent::ToolUseStarted {
            id: "t1".into(),
            name: "satz_questions".into(),
        });
        s.apply(AgentEvent::ToolUseStarted {
            id: "t2".into(),
            name: "satz_packs".into(),
        });
        assert!(s.has_open_call());
        s.debug_stderr("reading the estate".into());
        s.apply(AgentEvent::ToolResult {
            id: "t1".into(),
            name: "satz_questions".into(),
            outcome: outcome(None, "ok", false),
            millis: 1,
        });
        s.debug_stderr("reading the packs".into());
        s.apply(AgentEvent::Cancelled);
        assert!(!s.has_open_call(), "a call of an ended turn takes no line");
        s.debug_stderr("after the turn".into());
        assert_eq!(
            s.debug_entry("t1").unwrap().stderr,
            StderrLines::Lines(vec!["reading the estate".into()])
        );
        assert_eq!(
            s.debug_entry("t2").unwrap().stderr,
            StderrLines::Lines(vec!["reading the packs".into()])
        );

        // Claude Code's calls run on its own satz mcp: the estate's stderr is not theirs
        let mut cc = ChatStore::new(String::new(), Effort::High, EngineKind::ClaudeCode);
        started(&mut cc, "one");
        cc.apply(AgentEvent::ToolUseStarted {
            id: "t1".into(),
            name: "satz_questions".into(),
        });
        assert!(!cc.has_open_call());
        cc.debug_stderr("a line".into());
        assert_eq!(
            cc.debug_entry("t1").unwrap().stderr,
            StderrLines::Unseen(CLAUDE_CODE_STDERR)
        );
    }

    #[test]
    fn a_turn_names_its_model_and_adds_up_every_request() {
        let mut s = store();
        started(&mut s, "one");
        s.apply(AgentEvent::RequestDone {
            usage: usage(100, 10, Some(0)),
        });
        s.apply(AgentEvent::Started {
            model: "claude-sonnet-5".into(),
        });
        s.apply(AgentEvent::RequestDone {
            usage: usage(200, 20, Some(90)),
        });
        let live = s.streaming.as_ref().unwrap();
        assert_eq!(live.requests, 2);
        assert_eq!(live.model.as_deref(), Some("claude-sonnet-5"));
        assert_eq!(
            live.usage,
            Some(usage(100, 10, Some(0)).plus(usage(200, 20, Some(90))))
        );
        assert_eq!(
            s.model, "claude-opus-5",
            "the API engine keeps the model it was asked for"
        );

        let mut cc = ChatStore::new(
            "the Claude Code default".into(),
            Effort::High,
            EngineKind::ClaudeCode,
        );
        started(&mut cc, "one");
        assert_eq!(
            cc.model, "claude-opus-5",
            "Claude Code's model shows as the turn starts"
        );
    }

    #[test]
    fn turn_done_moves_streaming_into_turns_and_adds_the_usage() {
        let mut s = store();
        started(&mut s, "one");
        s.apply(AgentEvent::TextDelta("answer".into()));
        s.apply(AgentEvent::TurnDone {
            stop_reason: StopReason::EndTurn,
            usage: usage(100, 10, None),
        });
        assert!(s.streaming.is_none());
        assert!(!s.busy);
        assert_eq!(s.turns.len(), 2);
        let TurnView::Assistant(turn) = &s.turns[1] else {
            panic!("the second turn is the assistant's");
        };
        assert_eq!(turn.blocks, vec![Block::Text("answer".into())]);
        assert_eq!(turn.stop_reason, Some(StopReason::EndTurn));
        assert_eq!(turn.usage, Some(usage(100, 10, None)));

        started(&mut s, "two");
        s.apply(AgentEvent::TurnDone {
            stop_reason: StopReason::MaxTokens,
            usage: usage(200, 20, Some(0)),
        });
        assert_eq!(s.usage.turns, 2);
        assert_eq!(s.usage.turn, usage(200, 20, Some(0)));
        assert_eq!(s.usage.session.input_tokens, 300);
        assert_eq!(s.usage.session.output_tokens, 30);
        // a zero cache read is a reported zero, never dropped
        assert_eq!(s.usage.session.cache_read_input_tokens, Some(0));
        assert_eq!(s.usage.session.cache_creation_input_tokens, Some(0));
    }

    #[test]
    fn a_provider_that_never_reports_cache_figures_leaves_them_unreported() {
        let mut totals = UsageTotals::default();
        totals.add(Usage {
            input_tokens: 5,
            output_tokens: 1,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
        });
        assert_eq!(totals.session.cache_read_input_tokens, None);
        totals.add(Usage {
            input_tokens: 5,
            output_tokens: 1,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: Some(3),
        });
        assert_eq!(totals.session.cache_read_input_tokens, Some(3));
        assert_eq!(totals.session.input_tokens, 10);
    }

    #[test]
    fn refused_and_failed_and_cancelled_leave_turns_as_before() {
        let mut s = store();
        started(&mut s, "one");
        s.apply(AgentEvent::TurnDone {
            stop_reason: StopReason::EndTurn,
            usage: usage(1, 1, None),
        });
        let before = s.turns.clone();

        started(&mut s, "two");
        s.apply(AgentEvent::TextDelta("partial".into()));
        s.apply(AgentEvent::Refused {
            category: Some("c".into()),
            explanation: Some("e".into()),
            recommended_model: Some("claude-sonnet-5".into()),
        });
        assert_eq!(s.turns, before);
        assert!(s.streaming.is_none());
        assert!(!s.busy);
        assert_eq!(
            s.notice,
            Some(Notice::Refused {
                user_text: "two".into(),
                category: Some("c".into()),
                explanation: Some("e".into()),
                recommended_model: Some("claude-sonnet-5".into()),
            })
        );

        started(&mut s, "three");
        s.apply(AgentEvent::Failed(ClaudeError::Auth("bad key".into())));
        assert_eq!(s.turns, before);
        assert_eq!(
            s.notice,
            Some(Notice::Failed {
                user_text: "three".into(),
                error: "authentication failed: bad key".into(),
            })
        );

        started(&mut s, "four");
        s.apply(AgentEvent::Cancelled);
        assert_eq!(s.turns, before);
        assert_eq!(
            s.notice,
            Some(Notice::Cancelled {
                user_text: "four".into()
            })
        );
        assert_eq!(s.usage.turns, 1);
        assert_eq!(s.error, None);
    }

    #[test]
    fn a_notice_reaches_the_footer_and_leaves_the_turn_alone() {
        let mut s = store();
        started(&mut s, "one");
        s.apply(AgentEvent::TextDelta("partial".into()));
        s.apply(AgentEvent::Notice(
            "Claude plan: 51% of the seven-day limit used".into(),
        ));
        assert_eq!(
            s.engine_notice.as_deref(),
            Some("Claude plan: 51% of the seven-day limit used")
        );
        assert!(s.busy);
        assert_eq!(s.error, None);
        assert_eq!(
            s.streaming.as_ref().unwrap().blocks,
            vec![Block::Text("partial".into())]
        );
    }

    #[test]
    fn a_delta_or_an_end_outside_a_turn_is_an_error_not_a_panic() {
        let mut s = store();
        s.apply(AgentEvent::TextDelta("x".into()));
        assert_eq!(s.error.as_deref(), Some("TextDelta arrived outside a turn"));
        let mut s = store();
        s.apply(AgentEvent::Cancelled);
        assert_eq!(s.error.as_deref(), Some("the turn ended before it began"));
        let mut s = store();
        s.begin_turn("x".into());
        s.turn_closed();
        assert!(!s.busy);
        assert!(matches!(s.notice, Some(Notice::Failed { .. })));
    }

    #[test]
    fn the_delta_fast_path_matches_the_reducer() {
        let mut streaming = Some(AssistantTurn::default());
        assert!(is_delta(&AgentEvent::TextDelta("a".into())));
        assert!(!is_delta(&AgentEvent::Started {
            model: "claude-opus-5".into()
        }));
        apply_delta(&mut streaming, AgentEvent::TextDelta("a".into())).unwrap();
        apply_delta(&mut streaming, AgentEvent::TextDelta("b".into())).unwrap();
        assert_eq!(
            streaming.as_ref().unwrap().blocks,
            vec![Block::Text("ab".into())]
        );
        assert_eq!(
            apply_delta(
                &mut streaming,
                AgentEvent::Started {
                    model: "claude-opus-5".into()
                }
            )
            .unwrap_err(),
            "Started is not a delta"
        );
        assert_eq!(
            apply_delta(&mut None, AgentEvent::TextDelta("a".into())).unwrap_err(),
            "TextDelta arrived outside a turn"
        );
    }

    fn node(
        kind: ResourceKind,
        key: &str,
        tf_type: Option<&str>,
        label: Option<&str>,
    ) -> ResourceNode {
        ResourceNode {
            id: 0,
            kind,
            key: key.to_string(),
            key_parts: Vec::new(),
            label: label.map(str::to_string),
            label_parts: None,
            tf_type: tf_type.map(str::to_string),
            line: 1,
            attrs: Vec::new(),
            children: Vec::new(),
            uses: Vec::new(),
            missing_required: Vec::new(),
        }
    }

    #[test]
    fn the_context_renders_the_outline_the_questions_and_the_diagnostics() {
        let mut map = node(
            ResourceKind::ResourceMap,
            "google_folder",
            Some("google_folder"),
            None,
        );
        let mut infra = node(ResourceKind::Resource, "infra", Some("google_folder"), None);
        infra
            .children
            .push(node(ResourceKind::NestedBlock, "lifecycle", None, None));
        infra.children.push(node(
            ResourceKind::Resource,
            "google_project",
            Some("google_project"),
            Some("acme-infra"),
        ));
        map.children.push(infra);
        let model = EstateModel {
            main: PathBuf::from("/estates/acme/C0example.satz"),
            outline: vec![
                node(ResourceKind::Config, "terraform", None, None),
                map,
                node(ResourceKind::Unknown, "mystery", None, Some("x")),
            ],
            params: Vec::new(),
            packs: satz_studio_core::satz::reports::PacksReport {
                estate: "C0example.satz".into(),
                note: None,
                packs: Vec::new(),
                unmanaged: Vec::new(),
                findings: Vec::new(),
            },
            phases: Default::default(),
            uses: Vec::new(),
            hcl: Vec::new(),
            diagnostics: Vec::new(),
            schema: satz_studio_core::model::SchemaStatus::Missing(PathBuf::from(
                "/estates/acme/schema",
            )),
        };
        let questions = QuestionsReport {
            estate: "C0example.satz".into(),
            questions: Vec::new(),
            summary: QuestionsSummary {
                total: 16,
                answered: 12,
                unanswered: 4,
                not_applicable: 0,
                blocking: 1,
                one_way_doors: 2,
                complete: false,
            },
        };
        let diagnostics = vec![
            Diagnostic::error("unknown key `foo`\n  in block bar", DiagSource::Parse)
                .at("/estates/acme/C0example.satz", 12),
            Diagnostic::error(
                "questions failed",
                DiagSource::Tool("satz_questions".into()),
            ),
        ];
        let context = estate_context(&ContextInput {
            main: Path::new("/estates/acme/C0example.satz"),
            dir: Path::new("/estates/acme"),
            runs_as: Some("svc-iac@acme-infra.iam.gserviceaccount.com"),
            deployment_mode: Some("cloud"),
            model: Some(&model),
            questions: Some(&questions),
            diagnostics: &diagnostics,
        });
        assert_eq!(
            context.outline,
            vec![
                "terraform",
                "google_folder",
                "  google_folder \"infra\"",
                "    google_project \"acme-infra\"",
                "mystery \"x\"",
            ]
        );
        assert_eq!(
            context.questions_summary.as_deref(),
            Some("12 of 16 answered, 4 unanswered (1 blocking), 0 not applicable; incomplete")
        );
        assert_eq!(
            context.diagnostics,
            vec![
                "error: C0example.satz:12: unknown key `foo`",
                "error: questions failed"
            ]
        );
        let rendered = context.render();
        assert!(rendered.starts_with("estate: /estates/acme/C0example.satz\n"));
        assert!(rendered.contains("runs as: svc-iac@acme-infra.iam.gserviceaccount.com"));
        assert!(rendered.contains("deployment mode: cloud"));
        assert!(rendered.contains("\n      google_project \"acme-infra\""));
    }

    #[test]
    fn an_estate_without_a_model_renders_the_path_alone() {
        let context = estate_context(&ContextInput {
            main: Path::new("/estates/acme/C0example.satz"),
            dir: Path::new("/estates/acme"),
            runs_as: None,
            deployment_mode: None,
            model: None,
            questions: None,
            diagnostics: &[],
        });
        assert_eq!(context.render(), "estate: /estates/acme/C0example.satz");
    }

    #[test]
    fn replay_folds_a_tool_loop_into_one_assistant_turn() {
        let messages = vec![
            Message::user(vec![ContentBlock::text("check the estate")]),
            Message::assistant(vec![
                ContentBlock::Thinking {
                    thinking: "I should compile".into(),
                    signature: "sig".into(),
                },
                ContentBlock::text("Compiling."),
                ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "satz_transpile_check".into(),
                    input: serde_json::json!({}),
                },
            ]),
            Message::user(vec![ContentBlock::ToolResult {
                tool_use_id: "t1".into(),
                content: "{\n  \"addresses\": []\n}".into(),
                is_error: false,
                cache_control: None,
            }]),
            Message::assistant(vec![
                ContentBlock::Other(
                    serde_json::json!({"type": "fallback", "from": "claude-opus-5", "to": "claude-sonnet-5"}),
                ),
                ContentBlock::text("It compiles."),
                ContentBlock::Other(serde_json::json!({"type": "citation"})),
            ]),
            Message::user(vec![ContentBlock::text("thanks")]),
            Message::assistant(vec![
                ContentBlock::RedactedThinking { data: "x".into() },
                ContentBlock::text("Welcome."),
            ]),
        ];
        let Replayed { turns, debug } = replay(&messages).unwrap();
        assert_eq!(turns.len(), 4);
        assert_eq!(
            turns[0],
            TurnView::User {
                text: "check the estate".into()
            }
        );
        let TurnView::Assistant(first) = &turns[1] else {
            panic!("an assistant turn");
        };
        assert_eq!(first.requests, 2);
        assert_eq!(first.usage, None);
        assert_eq!(first.blocks.len(), 6);
        assert_eq!(first.blocks[0], Block::Thinking("I should compile".into()));
        assert_eq!(first.blocks[1], Block::Text("Compiling.".into()));
        let Block::Tool(card) = &first.blocks[2] else {
            panic!("a tool card");
        };
        assert_eq!(
            card.result,
            Some(ToolResultView {
                summary: "addresses: 0".into(),
                is_error: false,
                millis: None,
            })
        );
        assert!(card.texts().iter().all(|t| !t.contains('{')));
        // the log holds what a live turn would: the call's input and result under its id
        assert_eq!(debug.len(), 1);
        let entry = &debug[0];
        assert_eq!(entry.id, "t1");
        assert_eq!(entry.name, "satz_transpile_check");
        assert_eq!(entry.input.as_deref(), Some("{}"));
        assert_eq!(
            entry.result,
            Some(DebugResult {
                body: "{\n  \"addresses\": []\n}".into(),
                is_error: false,
                millis: None,
            })
        );
        assert_eq!(entry.stderr, StderrLines::Unseen(REPLAYED_STDERR));
        assert_eq!(
            first.blocks[3],
            Block::Fallback {
                from: "claude-opus-5".into(),
                to: "claude-sonnet-5".into()
            }
        );
        assert_eq!(first.blocks[4], Block::Text("It compiles.".into()));
        assert_eq!(first.blocks[5], Block::Other("citation".into()));
        assert_eq!(
            turns[2],
            TurnView::User {
                text: "thanks".into()
            }
        );
        let TurnView::Assistant(second) = &turns[3] else {
            panic!("an assistant turn");
        };
        assert_eq!(
            second.blocks,
            vec![Block::RedactedThinking, Block::Text("Welcome.".into())]
        );
    }

    #[test]
    fn replay_refuses_what_it_cannot_show_by_message_number() {
        let orphan = vec![Message::user(vec![ContentBlock::ToolResult {
            tool_use_id: "t9".into(),
            content: String::new(),
            is_error: false,
            cache_control: None,
        }])];
        assert_eq!(
            replay(&orphan).unwrap_err(),
            "message 1: a result for t9 without its call"
        );
        let image = vec![Message::user(vec![ContentBlock::Other(
            serde_json::json!({"type": "image"}),
        )])];
        assert_eq!(
            replay(&image).unwrap_err(),
            "message 1: a image block in a user message"
        );
    }

    #[test]
    fn a_new_or_resumed_conversation_replaces_everything() {
        let mut s = store();
        started(&mut s, "one");
        s.apply(AgentEvent::TurnDone {
            stop_reason: StopReason::EndTurn,
            usage: usage(1, 1, None),
        });
        s.error = Some("stale".into());
        s.load_conversation(
            Replayed {
                turns: vec![TurnView::User { text: "old".into() }],
                debug: Vec::new(),
            },
            PathBuf::from("/t/1.jsonl"),
            "claude-sonnet-5".into(),
        );
        assert_eq!(s.turns, vec![TurnView::User { text: "old".into() }]);
        assert_eq!(s.transcript, Some(PathBuf::from("/t/1.jsonl")));
        assert_eq!(s.model, "claude-sonnet-5");
        assert_eq!(s.usage, UsageTotals::default());
        assert_eq!(s.error, None);
        s.reset_conversation("claude-opus-5".into());
        assert!(s.turns.is_empty());
        assert_eq!(s.transcript, None);
    }
}
