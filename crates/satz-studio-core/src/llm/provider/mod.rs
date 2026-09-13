//! The providers that are not Claude. Each maps the Claude-shaped [`Request`] to its
//! wire format and its stream back into [`StreamEvent`]s; what a provider cannot carry
//! — thinking, effort, cache breakpoints — is dropped, and its [`Capabilities`] say so.
//!
//! [`Request`]: super::Request
//! [`StreamEvent`]: super::StreamEvent
//! [`Capabilities`]: super::Capabilities

pub mod ollama;
pub mod openai_compat;

use std::collections::BTreeMap;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::StreamEvent;
use super::claude::error::{ClaudeError, from_status};
use super::claude::types::{ContentBlock, Message, Role, StopReason, ToolDef};

/// The whole request, connection to the last body byte.
pub const TIMEOUT: Duration = Duration::from_secs(600);

pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder().timeout(TIMEOUT).build().expect("the HTTP client builds: no proxy or TLS setting of this app can fail")
}

/// Send a request and hand its body back chunk by chunk; a non-2xx status is the
/// mapped error, a transport failure before the body a connection error.
pub(crate) async fn post_stream(request: reqwest::RequestBuilder, cancel: &CancellationToken) -> Result<reqwest::Response, ClaudeError> {
    let response = tokio::select! {
        r = request.send() => r.map_err(|e| ClaudeError::Connection(e.to_string()))?,
        _ = cancel.cancelled() => return Err(ClaudeError::Cancelled),
    };
    let status = response.status();
    if !status.is_success() {
        let headers = response.headers().clone();
        let text = response.text().await.map_err(|e| ClaudeError::Connection(e.to_string()))?;
        return Err(from_status(status, &headers, &text));
    }
    Ok(response)
}

/// Send one event; a closed receiver means the caller is gone.
pub(crate) async fn emit(tx: &mpsc::Sender<StreamEvent>, event: StreamEvent) -> Result<(), ClaudeError> {
    tx.send(event).await.map_err(|_| ClaudeError::Cancelled)
}

/// The `tools` array both adapters send: OpenAI's function shape, which Ollama shares.
pub(crate) fn function_tools(tools: &[ToolDef]) -> serde_json::Value {
    serde_json::Value::Array(tools.iter().map(|t| serde_json::json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.input_schema}})).collect())
}

/// The tool name behind every `tool_use` id in a transcript, for a provider whose tool
/// results are keyed by name.
pub(crate) fn tool_names(messages: &[Message]) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    for message in messages.iter().filter(|m| m.role == Role::Assistant) {
        for block in &message.content {
            if let ContentBlock::ToolUse { id, name, .. } = block {
                names.insert(id.clone(), name.clone());
            }
        }
    }
    names
}

/// The `content` of a tool message: an error is said to be one, since neither wire
/// format has a flag for it.
pub(crate) fn tool_content(content: &str, is_error: bool) -> String {
    if is_error { format!("error: {content}") } else { content.to_string() }
}

/// The blocks a provider's stream builds up: one open text block at a time, tool
/// calls in order, block indices assigned as blocks open.
#[derive(Default)]
pub(crate) struct Blocks {
    next_index: usize,
    text: Option<(usize, String)>,
    /// (block index, id, name, arguments so far)
    calls: Vec<(usize, String, String, String)>,
}

impl Blocks {
    /// Append text to the open text block, opening one when none is.
    pub(crate) fn text(&mut self, piece: &str) {
        match &mut self.text {
            Some((_, text)) => text.push_str(piece),
            None => {
                self.text = Some((self.next_index, piece.to_string()));
                self.next_index += 1;
            }
        }
    }

    /// Close the open text block, if any: the event that completes it.
    pub(crate) fn close_text(&mut self) -> Option<StreamEvent> {
        self.text.take().map(|(index, text)| StreamEvent::BlockStop { index, block: ContentBlock::Text { text, cache_control: None } })
    }

    /// Open a tool call; its block index.
    pub(crate) fn open_call(&mut self, id: String, name: String) -> usize {
        let index = self.next_index;
        self.next_index += 1;
        self.calls.push((index, id, name, String::new()));
        index
    }

    pub(crate) fn call_mut(&mut self, position: usize) -> Option<&mut (usize, String, String, String)> {
        self.calls.get_mut(position)
    }

    pub(crate) fn calls(&self) -> usize {
        self.calls.len()
    }

    /// Close every tool call: the arguments parsed (none is `{}`), one event each.
    pub(crate) fn close_calls(&mut self) -> Result<Vec<StreamEvent>, ClaudeError> {
        let mut events = Vec::new();
        for (index, id, name, arguments) in self.calls.drain(..) {
            let input = if arguments.trim().is_empty() {
                serde_json::Value::Object(Default::default())
            } else {
                serde_json::from_str(&arguments).map_err(|e| ClaudeError::Stream(format!("the arguments of tool `{name}` are not JSON: {e}")))?
            };
            events.push(StreamEvent::BlockStop { index, block: ContentBlock::ToolUse { id, name, input } });
        }
        Ok(events)
    }
}

/// The stop reason a finish reason maps to: `stop` is the end of the turn unless tool
/// calls were made, `length` is the token limit.
pub(crate) fn stop_reason(finish: Option<&str>, calls: usize) -> StopReason {
    match finish {
        _ if calls > 0 => StopReason::ToolUse,
        Some("stop") | None => StopReason::EndTurn,
        Some("tool_calls") => StopReason::ToolUse,
        Some("length") => StopReason::MaxTokens,
        Some("content_filter") => StopReason::Refusal,
        Some(_) => StopReason::Unknown,
    }
}
