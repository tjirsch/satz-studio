//! The `text/event-stream` decoder and the assembler that folds the Messages API's
//! stream events into a [`Response`] while emitting [`StreamEvent`]s.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::error::ClaudeError;
use super::types::{ContentBlock, Response, StopDetails, StopReason, Usage};
use crate::llm::StreamEvent;

/// Complete lines out of a byte stream: buffers across chunks, splits on `\n`, strips
/// a trailing `\r`, and refuses a line that is not UTF-8.
#[derive(Debug, Default)]
pub struct Lines {
    buf: Vec<u8>,
}

impl Lines {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, ClaudeError> {
        self.buf.extend_from_slice(chunk);
        let mut lines = Vec::new();
        while let Some(end) = self.buf.iter().position(|b| *b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=end).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            lines.push(String::from_utf8(line).map_err(|e| {
                ClaudeError::Stream(format!("a line of the stream is not UTF-8: {e}"))
            })?);
        }
        Ok(lines)
    }

    /// Bytes after the last newline.
    pub fn pending(&self) -> &[u8] {
        &self.buf
    }
}

/// One server-sent event: its `event` name (`message` when absent) and the `data`
/// lines joined with `\n`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

/// An incremental `text/event-stream` decoder: feed it the body's chunks, take the
/// events each chunk completes. A blank line ends an event; `event:` and `data:` lines
/// are read, `id:`, `retry:` and comment lines are ignored.
#[derive(Debug, Default)]
pub struct SseDecoder {
    lines: Lines,
    event: Option<String>,
    data: Vec<String>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, ClaudeError> {
        let mut events = Vec::new();
        for line in self.lines.push(chunk)? {
            if line.is_empty() {
                if let Some(event) = self.take() {
                    events.push(event);
                }
                continue;
            }
            if line.starts_with(':') {
                continue;
            }
            let (field, value) = match line.split_once(':') {
                Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
                None => (line.as_str(), ""),
            };
            match field {
                "event" => self.event = Some(value.to_string()),
                "data" => self.data.push(value.to_string()),
                _ => {}
            }
        }
        Ok(events)
    }

    /// The end of the body: an event left without its terminating blank line, or bytes
    /// without their newline, mean the stream was cut.
    pub fn finish(self) -> Result<(), ClaudeError> {
        if !self.lines.pending().is_empty() || self.event.is_some() || !self.data.is_empty() {
            return Err(ClaudeError::Stream(
                "the stream ended inside an event".to_string(),
            ));
        }
        Ok(())
    }

    fn take(&mut self) -> Option<SseEvent> {
        let event = self.event.take();
        if self.data.is_empty() {
            return None;
        }
        let data = std::mem::take(&mut self.data).join("\n");
        Some(SseEvent {
            event: event.unwrap_or_else(|| "message".to_string()),
            data,
        })
    }
}

/// The stream's events as the API sends them in `data`.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Wire {
    MessageStart {
        message: WireMessage,
    },
    ContentBlockStart {
        index: usize,
        content_block: serde_json::Value,
    },
    ContentBlockDelta {
        index: usize,
        delta: serde_json::Value,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        delta: WireDelta,
        #[serde(default)]
        usage: Option<PartialUsage>,
    },
    MessageStop,
    Ping,
    Error {
        error: WireError,
    },
}

#[derive(Deserialize)]
struct WireMessage {
    id: String,
    model: String,
    #[serde(default)]
    usage: Usage,
}

#[derive(Deserialize)]
struct WireDelta {
    #[serde(default)]
    stop_reason: Option<StopReason>,
    #[serde(default)]
    stop_details: Option<StopDetails>,
}

/// `message_delta.usage`: the fields it carries override what `message_start` gave.
#[derive(Deserialize)]
struct PartialUsage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct WireError {
    #[serde(rename = "type")]
    kind: String,
    message: String,
}

/// A block while its deltas arrive.
enum Building {
    Text(String),
    Thinking {
        thinking: String,
        signature: String,
    },
    RedactedThinking(String),
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        json: String,
        deltas: bool,
    },
    Other(serde_json::Value),
}

/// Folds the SSE events of one response into a [`Response`] and yields the
/// [`StreamEvent`]s as they arrive. A tool's input is the `input_json_delta`s of its
/// block concatenated and parsed at `content_block_stop`, never string-matched.
pub struct Assembler {
    id: Option<String>,
    model: String,
    usage: Usage,
    stop_reason: Option<StopReason>,
    stop_details: Option<StopDetails>,
    blocks: Vec<Option<Building>>,
    done: Vec<ContentBlock>,
    open: BTreeMap<usize, ()>,
    stopped: bool,
}

impl Default for Assembler {
    fn default() -> Self {
        Self::new()
    }
}

impl Assembler {
    pub fn new() -> Self {
        Self {
            id: None,
            model: String::new(),
            usage: Usage::default(),
            stop_reason: None,
            stop_details: None,
            blocks: Vec::new(),
            done: Vec::new(),
            open: BTreeMap::new(),
            stopped: false,
        }
    }

    /// `message_start` has arrived.
    pub fn started(&self) -> bool {
        self.id.is_some()
    }

    /// Fold one event; the stream events it produces, in order.
    pub fn feed(&mut self, event: &SseEvent) -> Result<Vec<StreamEvent>, ClaudeError> {
        let wire: Wire = serde_json::from_str(&event.data).map_err(|e| {
            ClaudeError::Stream(format!(
                "event `{}` is not one this code reads: {e}",
                event.event
            ))
        })?;
        let mut out = Vec::new();
        match wire {
            Wire::MessageStart { message } => {
                self.id = Some(message.id.clone());
                self.model = message.model.clone();
                self.usage = message.usage;
                out.push(StreamEvent::Started {
                    id: message.id,
                    model: message.model,
                });
            }
            Wire::ContentBlockStart {
                index,
                content_block,
            } => {
                if index != self.blocks.len() {
                    return Err(ClaudeError::Stream(format!(
                        "content block {index} started where {} was expected",
                        self.blocks.len()
                    )));
                }
                let kind = content_block
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let building = match kind.as_str() {
                    "text" => {
                        let text = field_str(&content_block, "text");
                        if !text.is_empty() {
                            out.push(StreamEvent::TextDelta(text.clone()));
                        }
                        Building::Text(text)
                    }
                    "thinking" => Building::Thinking {
                        thinking: field_str(&content_block, "thinking"),
                        signature: field_str(&content_block, "signature"),
                    },
                    "redacted_thinking" => {
                        Building::RedactedThinking(field_str(&content_block, "data"))
                    }
                    "tool_use" => {
                        let id = field_str(&content_block, "id");
                        let name = field_str(&content_block, "name");
                        let input = content_block
                            .get("input")
                            .cloned()
                            .unwrap_or(serde_json::Value::Object(Default::default()));
                        out.push(StreamEvent::ToolUseStart {
                            index,
                            id: id.clone(),
                            name: name.clone(),
                        });
                        Building::ToolUse {
                            id,
                            name,
                            input,
                            json: String::new(),
                            deltas: false,
                        }
                    }
                    _ => Building::Other(content_block),
                };
                self.blocks.push(Some(building));
                self.open.insert(index, ());
            }
            Wire::ContentBlockDelta { index, delta } => {
                let kind = delta
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let block = self.building_mut(index)?;
                match (kind.as_str(), block) {
                    ("text_delta", Building::Text(text)) => {
                        let piece = field_str(&delta, "text");
                        text.push_str(&piece);
                        out.push(StreamEvent::TextDelta(piece));
                    }
                    ("thinking_delta", Building::Thinking { thinking, .. }) => {
                        let piece = field_str(&delta, "thinking");
                        thinking.push_str(&piece);
                        out.push(StreamEvent::ThinkingDelta(piece));
                    }
                    ("signature_delta", Building::Thinking { signature, .. }) => {
                        signature.push_str(&field_str(&delta, "signature"))
                    }
                    ("input_json_delta", Building::ToolUse { json, deltas, .. }) => {
                        let piece = field_str(&delta, "partial_json");
                        json.push_str(&piece);
                        *deltas = true;
                        out.push(StreamEvent::ToolInputDelta {
                            index,
                            partial_json: piece,
                        });
                    }
                    (other, _) => {
                        return Err(ClaudeError::Stream(format!(
                            "delta `{other}` on content block {index} is not one this code folds"
                        )));
                    }
                }
            }
            Wire::ContentBlockStop { index } => {
                let block = self.building_mut(index)?;
                let done = match std::mem::replace(block, Building::Text(String::new())) {
                    Building::Text(text) => ContentBlock::Text {
                        text,
                        cache_control: None,
                    },
                    Building::Thinking {
                        thinking,
                        signature,
                    } => ContentBlock::Thinking {
                        thinking,
                        signature,
                    },
                    Building::RedactedThinking(data) => ContentBlock::RedactedThinking { data },
                    Building::ToolUse {
                        id,
                        name,
                        input,
                        json,
                        deltas,
                    } => {
                        let input = if deltas {
                            serde_json::from_str(&json).map_err(|e| {
                                ClaudeError::Stream(format!(
                                    "the input of tool `{name}` is not JSON: {e}"
                                ))
                            })?
                        } else {
                            input
                        };
                        ContentBlock::ToolUse { id, name, input }
                    }
                    Building::Other(value) => ContentBlock::Other(value),
                };
                self.blocks[index] = None;
                self.open.remove(&index);
                if index != self.done.len() {
                    return Err(ClaudeError::Stream(format!(
                        "content block {index} stopped before block {}",
                        self.done.len()
                    )));
                }
                self.done.push(done.clone());
                out.push(StreamEvent::BlockStop { index, block: done });
            }
            Wire::MessageDelta { delta, usage } => {
                if delta.stop_reason.is_some() {
                    self.stop_reason = delta.stop_reason;
                }
                if delta.stop_details.is_some() {
                    self.stop_details = delta.stop_details;
                }
                if let Some(partial) = usage {
                    if let Some(n) = partial.input_tokens {
                        self.usage.input_tokens = n;
                    }
                    if let Some(n) = partial.output_tokens {
                        self.usage.output_tokens = n;
                    }
                    if partial.cache_creation_input_tokens.is_some() {
                        self.usage.cache_creation_input_tokens =
                            partial.cache_creation_input_tokens;
                    }
                    if partial.cache_read_input_tokens.is_some() {
                        self.usage.cache_read_input_tokens = partial.cache_read_input_tokens;
                    }
                }
            }
            Wire::MessageStop => {
                if !self.open.is_empty() {
                    return Err(ClaudeError::Stream(format!(
                        "message_stop with content block {} still open",
                        self.open.keys().next().expect("non-empty")
                    )));
                }
                let stop_reason = self.stop_reason.ok_or_else(|| {
                    ClaudeError::Stream("message_stop without a stop_reason".to_string())
                })?;
                self.stopped = true;
                out.push(StreamEvent::Done {
                    stop_reason,
                    stop_details: self.stop_details.clone(),
                    usage: self.usage,
                });
            }
            Wire::Ping => {}
            Wire::Error { error } => {
                return Err(ClaudeError::Stream(format!(
                    "{}: {}",
                    error.kind, error.message
                )));
            }
        }
        Ok(out)
    }

    /// The response, once `message_stop` has been folded.
    pub fn finish(self) -> Result<Response, ClaudeError> {
        let id = self.id.ok_or_else(|| {
            ClaudeError::Stream("the stream ended before message_start".to_string())
        })?;
        if !self.stopped {
            return Err(ClaudeError::Stream(
                "the stream ended before message_stop".to_string(),
            ));
        }
        let stop_reason = self
            .stop_reason
            .ok_or_else(|| ClaudeError::Stream("message_stop without a stop_reason".to_string()))?;
        Ok(Response {
            id,
            model: self.model,
            content: self.done,
            stop_reason,
            stop_details: self.stop_details,
            usage: self.usage,
        })
    }

    fn building_mut(&mut self, index: usize) -> Result<&mut Building, ClaudeError> {
        match self.blocks.get_mut(index) {
            Some(Some(block)) => Ok(block),
            Some(None) => Err(ClaudeError::Stream(format!(
                "content block {index} is already stopped"
            ))),
            None => Err(ClaudeError::Stream(format!(
                "content block {index} was never started"
            ))),
        }
    }
}

fn field_str(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}
