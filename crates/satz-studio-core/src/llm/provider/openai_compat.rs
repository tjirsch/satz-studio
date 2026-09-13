//! Any endpoint speaking the OpenAI chat-completions API: Ollama's compatible endpoint,
//! LM Studio, OpenRouter, Gemini's compatible endpoint.

use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{Blocks, emit, function_tools, http_client, post_stream, stop_reason, tool_content};
use crate::llm::claude::error::ClaudeError;
use crate::llm::claude::sse::SseDecoder;
use crate::llm::claude::types::{ContentBlock, Request, Role, StopDetails, Usage};
use crate::llm::{Capabilities, ChatProvider, StreamEvent, StreamFuture};

/// `POST {base_url}/chat/completions`, streamed. The provider's own `model` is sent —
/// the request's names a Claude model.
#[derive(Debug, Clone)]
pub struct OpenAiCompat {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    http: reqwest::Client,
}

impl OpenAiCompat {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key,
            model: model.into(),
            http: http_client(),
        }
    }

    /// The wire body: the system blocks joined into one system message, tool results
    /// as `tool` messages, `tool_use` blocks as `tool_calls`; thinking, effort and cache
    /// breakpoints dropped.
    pub fn body(&self, req: &Request) -> serde_json::Value {
        let mut messages = Vec::new();
        let system: Vec<&str> = req.system.iter().map(|s| s.text.as_str()).collect();
        if !system.is_empty() {
            messages.push(serde_json::json!({"role": "system", "content": system.join("\n\n")}));
        }
        for message in &req.messages {
            match message.role {
                Role::User => {
                    for block in &message.content {
                        if let ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                            ..
                        } = block
                        {
                            messages.push(serde_json::json!({"role": "tool", "tool_call_id": tool_use_id, "content": tool_content(content, *is_error)}));
                        }
                    }
                    let text = message.text();
                    if !text.is_empty() {
                        messages.push(serde_json::json!({"role": "user", "content": text}));
                    }
                }
                Role::Assistant => {
                    let text = message.text();
                    let calls: Vec<serde_json::Value> = message
                        .content
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::ToolUse { id, name, input } => Some(serde_json::json!({"id": id, "type": "function", "function": {"name": name, "arguments": input.to_string()}})),
                            _ => None,
                        })
                        .collect();
                    let mut assistant = serde_json::json!({"role": "assistant", "content": if text.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(text) }});
                    if !calls.is_empty() {
                        assistant["tool_calls"] = serde_json::Value::Array(calls);
                    }
                    messages.push(assistant);
                }
            }
        }
        let mut body = serde_json::json!({"model": self.model, "stream": true, "stream_options": {"include_usage": true}, "messages": messages});
        if !req.tools.is_empty() {
            body["tools"] = function_tools(&req.tools);
        }
        body
    }

    async fn run(
        &self,
        req: &Request,
        tx: mpsc::Sender<StreamEvent>,
        cancel: CancellationToken,
    ) -> Result<(), ClaudeError> {
        match self.run_inner(req, &tx, &cancel).await {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                Err(e)
            }
        }
    }

    async fn run_inner(
        &self,
        req: &Request,
        tx: &mpsc::Sender<StreamEvent>,
        cancel: &CancellationToken,
    ) -> Result<(), ClaudeError> {
        let mut request = self
            .http
            .post(format!(
                "{}/chat/completions",
                self.base_url.trim_end_matches('/')
            ))
            .json(&self.body(req));
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let response = post_stream(request, cancel).await?;
        let mut stream = response.bytes_stream();
        let mut decoder = SseDecoder::new();
        let mut fold = Fold::default();
        loop {
            let chunk = tokio::select! {
                c = stream.next() => c,
                _ = cancel.cancelled() => return Err(ClaudeError::Cancelled),
            };
            match chunk {
                Some(Ok(bytes)) => {
                    for event in decoder.push(bytes.as_ref())? {
                        if event.data.trim() == "[DONE]" {
                            for e in fold.finish()? {
                                emit(tx, e).await?;
                            }
                            return Ok(());
                        }
                        for e in fold.feed(&event.data)? {
                            emit(tx, e).await?;
                        }
                    }
                }
                Some(Err(e)) => return Err(ClaudeError::Stream(e.to_string())),
                None => break,
            }
        }
        decoder.finish()?;
        for e in fold.finish()? {
            emit(tx, e).await?;
        }
        Ok(())
    }
}

impl ChatProvider for OpenAiCompat {
    fn id(&self) -> &str {
        "openai-compat"
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            tools: true,
            thinking: false,
            effort: false,
            cache_control: false,
        }
    }
    fn stream<'a>(
        &'a self,
        req: &'a Request,
        tx: mpsc::Sender<StreamEvent>,
        cancel: CancellationToken,
    ) -> StreamFuture<'a> {
        Box::pin(self.run(req, tx, cancel))
    }
}

#[derive(Deserialize)]
struct Chunk {
    #[serde(default)]
    id: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<ChunkUsage>,
}

#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    delta: Delta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(Deserialize)]
struct ToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<FunctionDelta>,
}

#[derive(Deserialize)]
struct FunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct ChunkUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

/// The chunks of one completion folded into blocks.
#[derive(Default)]
struct Fold {
    started: bool,
    blocks: Blocks,
    finish: Option<String>,
    usage: Usage,
    finished: bool,
}

impl Fold {
    fn feed(&mut self, data: &str) -> Result<Vec<StreamEvent>, ClaudeError> {
        let chunk: Chunk = serde_json::from_str(data)
            .map_err(|e| ClaudeError::Stream(format!("a chunk is not a chat completion: {e}")))?;
        let mut out = Vec::new();
        if !self.started {
            self.started = true;
            out.push(StreamEvent::Started {
                id: chunk.id,
                model: chunk.model,
            });
        }
        if let Some(usage) = chunk.usage {
            self.usage.input_tokens = usage.prompt_tokens;
            self.usage.output_tokens = usage.completion_tokens;
        }
        for choice in chunk.choices {
            if let Some(text) = choice.delta.content
                && !text.is_empty()
            {
                self.blocks.text(&text);
                out.push(StreamEvent::TextDelta(text));
            }
            for call in choice.delta.tool_calls.unwrap_or_default() {
                let arguments = call
                    .function
                    .as_ref()
                    .and_then(|f| f.arguments.clone())
                    .unwrap_or_default();
                if call.index < self.blocks.calls() {
                    let index = self.blocks.call_mut(call.index).expect("index checked").0;
                    if !arguments.is_empty() {
                        self.blocks
                            .call_mut(call.index)
                            .expect("index checked")
                            .3
                            .push_str(&arguments);
                        out.push(StreamEvent::ToolInputDelta {
                            index,
                            partial_json: arguments,
                        });
                    }
                    continue;
                }
                if call.index != self.blocks.calls() {
                    return Err(ClaudeError::Stream(format!(
                        "tool call {} started where {} was expected",
                        call.index,
                        self.blocks.calls()
                    )));
                }
                let id = call.id.ok_or_else(|| {
                    ClaudeError::Stream(format!("tool call {} started without an id", call.index))
                })?;
                let name = call
                    .function
                    .as_ref()
                    .and_then(|f| f.name.clone())
                    .ok_or_else(|| {
                        ClaudeError::Stream(format!(
                            "tool call {} started without a name",
                            call.index
                        ))
                    })?;
                out.extend(self.blocks.close_text());
                let index = self.blocks.open_call(id.clone(), name.clone());
                out.push(StreamEvent::ToolUseStart { index, id, name });
                if !arguments.is_empty() {
                    self.blocks
                        .call_mut(call.index)
                        .expect("just opened")
                        .3
                        .push_str(&arguments);
                    out.push(StreamEvent::ToolInputDelta {
                        index,
                        partial_json: arguments,
                    });
                }
            }
            if choice.finish_reason.is_some() {
                self.finish = choice.finish_reason;
            }
        }
        Ok(out)
    }

    /// The end of the stream: the open blocks completed, then `Done`.
    fn finish(&mut self) -> Result<Vec<StreamEvent>, ClaudeError> {
        if self.finished {
            return Ok(Vec::new());
        }
        if !self.started {
            return Err(ClaudeError::Stream(
                "the stream ended without a chunk".to_string(),
            ));
        }
        self.finished = true;
        let mut out = Vec::new();
        out.extend(self.blocks.close_text());
        let calls = self.blocks.calls();
        out.extend(self.blocks.close_calls()?);
        let stop = stop_reason(self.finish.as_deref(), calls);
        let stop_details = (stop == StopReason::Refusal).then(|| StopDetails {
            kind: "refusal".to_string(),
            category: self.finish.clone(),
            explanation: None,
            recommended_model: None,
        });
        out.push(StreamEvent::Done {
            stop_reason: stop,
            stop_details,
            usage: self.usage,
        });
        Ok(out)
    }
}

use crate::llm::claude::types::StopReason;
