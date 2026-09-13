//! Ollama's own API: `POST {base_url}/api/chat`, an NDJSON stream.

use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{Blocks, emit, function_tools, http_client, post_stream, stop_reason, tool_content, tool_names};
use crate::llm::claude::error::ClaudeError;
use crate::llm::claude::sse::Lines;
use crate::llm::claude::types::{ContentBlock, Request, Role, Usage};
use crate::llm::{Capabilities, ChatProvider, StreamEvent, StreamFuture};

/// `POST {base_url}/api/chat`, streamed as NDJSON. The provider's own `model` is sent.
/// Ollama's tool calls carry no id: the adapter numbers them (`call_<n>`) and answers
/// a tool result by the tool's name.
#[derive(Debug, Clone)]
pub struct Ollama {
    pub base_url: String,
    pub model: String,
    http: reqwest::Client,
}

impl Ollama {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self { base_url: base_url.into(), model: model.into(), http: http_client() }
    }

    /// The wire body: the system blocks joined into one system message, tool results
    /// as `tool` messages named after their tool, `tool_use` blocks as `tool_calls`
    /// with their arguments as objects; thinking, effort and cache breakpoints dropped.
    pub fn body(&self, req: &Request) -> serde_json::Value {
        let names = tool_names(&req.messages);
        let mut messages = Vec::new();
        let system: Vec<&str> = req.system.iter().map(|s| s.text.as_str()).collect();
        if !system.is_empty() {
            messages.push(serde_json::json!({"role": "system", "content": system.join("\n\n")}));
        }
        for message in &req.messages {
            match message.role {
                Role::User => {
                    for block in &message.content {
                        if let ContentBlock::ToolResult { tool_use_id, content, is_error, .. } = block {
                            let mut tool = serde_json::json!({"role": "tool", "content": tool_content(content, *is_error)});
                            if let Some(name) = names.get(tool_use_id) {
                                tool["tool_name"] = serde_json::Value::String(name.clone());
                            }
                            messages.push(tool);
                        }
                    }
                    let text = message.text();
                    if !text.is_empty() {
                        messages.push(serde_json::json!({"role": "user", "content": text}));
                    }
                }
                Role::Assistant => {
                    let calls: Vec<serde_json::Value> = message
                        .content
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::ToolUse { name, input, .. } => Some(serde_json::json!({"function": {"name": name, "arguments": input}})),
                            _ => None,
                        })
                        .collect();
                    let mut assistant = serde_json::json!({"role": "assistant", "content": message.text()});
                    if !calls.is_empty() {
                        assistant["tool_calls"] = serde_json::Value::Array(calls);
                    }
                    messages.push(assistant);
                }
            }
        }
        let mut body = serde_json::json!({"model": self.model, "stream": true, "messages": messages});
        if !req.tools.is_empty() {
            body["tools"] = function_tools(&req.tools);
        }
        body
    }

    async fn run(&self, req: &Request, tx: mpsc::Sender<StreamEvent>, cancel: CancellationToken) -> Result<(), ClaudeError> {
        match self.run_inner(req, &tx, &cancel).await {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                Err(e)
            }
        }
    }

    async fn run_inner(&self, req: &Request, tx: &mpsc::Sender<StreamEvent>, cancel: &CancellationToken) -> Result<(), ClaudeError> {
        let request = self.http.post(format!("{}/api/chat", self.base_url.trim_end_matches('/'))).json(&self.body(req));
        let response = post_stream(request, cancel).await?;
        let mut stream = response.bytes_stream();
        let mut lines = Lines::new();
        let mut fold = Fold::default();
        loop {
            let chunk = tokio::select! {
                c = stream.next() => c,
                _ = cancel.cancelled() => return Err(ClaudeError::Cancelled),
            };
            match chunk {
                Some(Ok(bytes)) => {
                    for line in lines.push(bytes.as_ref())? {
                        if line.trim().is_empty() {
                            continue;
                        }
                        for e in fold.feed(&line)? {
                            emit(tx, e).await?;
                        }
                        if fold.done {
                            return Ok(());
                        }
                    }
                }
                Some(Err(e)) => return Err(ClaudeError::Stream(e.to_string())),
                None => break,
            }
        }
        if !lines.pending().is_empty() {
            return Err(ClaudeError::Stream("the stream ended inside a line".to_string()));
        }
        Err(ClaudeError::Stream("the stream ended before its `done` line".to_string()))
    }
}

impl ChatProvider for Ollama {
    fn id(&self) -> &str {
        "ollama"
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { tools: true, thinking: false, effort: false, cache_control: false }
    }
    fn stream<'a>(&'a self, req: &'a Request, tx: mpsc::Sender<StreamEvent>, cancel: CancellationToken) -> StreamFuture<'a> {
        Box::pin(self.run(req, tx, cancel))
    }
}

#[derive(Deserialize)]
struct Chunk {
    #[serde(default)]
    model: String,
    #[serde(default)]
    message: Option<ChunkMessage>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u64>,
    #[serde(default)]
    eval_count: Option<u64>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
struct ChunkMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Deserialize)]
struct ToolCall {
    function: Function,
}

#[derive(Deserialize)]
struct Function {
    name: String,
    #[serde(default)]
    arguments: serde_json::Value,
}

#[derive(Default)]
struct Fold {
    started: bool,
    blocks: Blocks,
    usage: Usage,
    done: bool,
}

impl Fold {
    fn feed(&mut self, line: &str) -> Result<Vec<StreamEvent>, ClaudeError> {
        let chunk: Chunk = serde_json::from_str(line).map_err(|e| ClaudeError::Stream(format!("a line is not an Ollama chat chunk: {e}")))?;
        if let Some(error) = chunk.error {
            return Err(ClaudeError::Stream(error));
        }
        let mut out = Vec::new();
        if !self.started {
            self.started = true;
            out.push(StreamEvent::Started { id: "ollama".to_string(), model: chunk.model });
        }
        if let Some(message) = chunk.message {
            if !message.content.is_empty() {
                self.blocks.text(&message.content);
                out.push(StreamEvent::TextDelta(message.content));
            }
            for call in message.tool_calls.unwrap_or_default() {
                out.extend(self.blocks.close_text());
                let id = format!("call_{}", self.blocks.calls());
                let index = self.blocks.open_call(id.clone(), call.function.name.clone());
                let arguments = call.function.arguments.to_string();
                self.blocks.call_mut(self.blocks.calls() - 1).expect("just opened").3 = arguments.clone();
                out.push(StreamEvent::ToolUseStart { index, id, name: call.function.name });
                out.push(StreamEvent::ToolInputDelta { index, partial_json: arguments });
            }
        }
        if let Some(n) = chunk.prompt_eval_count {
            self.usage.input_tokens = n;
        }
        if let Some(n) = chunk.eval_count {
            self.usage.output_tokens = n;
        }
        if chunk.done {
            self.done = true;
            out.extend(self.blocks.close_text());
            let calls = self.blocks.calls();
            out.extend(self.blocks.close_calls()?);
            out.push(StreamEvent::Done { stop_reason: stop_reason(chunk.done_reason.as_deref(), calls), stop_details: None, usage: self.usage });
        }
        Ok(out)
    }
}
