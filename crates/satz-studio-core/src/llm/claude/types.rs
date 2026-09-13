//! The Messages API wire types, used as the app's only message model. A transcript
//! is a list of [`Message`]s; a request to `POST /v1/messages` is [`body`] over a
//! [`Request`]; what the stream assembles is a [`Response`].

use serde::{Deserialize, Serialize};

use crate::llm::auth::Credential;

/// `output_config.effort`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub kind: String,
}

impl CacheControl {
    pub fn ephemeral() -> Self {
        Self { kind: "ephemeral".to_string() }
    }
}

/// The content blocks of the Messages API.
///
/// The five variants this code reads are typed; every other block — `fallback`, or a
/// type newer than this code — is [`ContentBlock::Other`] and carries the JSON exactly
/// as the API returned it, so a transcript replays it verbatim. Serialisation of the
/// typed variants is the API's shape (`{"type": "text", "text": …}`); `Other`
/// serialises as its value.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentBlock {
    Text {
        text: String,
        cache_control: Option<CacheControl>,
    },
    Thinking {
        thinking: String,
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
        is_error: bool,
        cache_control: Option<CacheControl>,
    },
    /// A block of a type this code does not read, verbatim.
    Other(serde_json::Value),
}

impl ContentBlock {
    pub fn text(text: impl Into<String>) -> Self {
        ContentBlock::Text { text: text.into(), cache_control: None }
    }

    /// The `type` of the block as the API names it.
    pub fn kind(&self) -> &str {
        match self {
            ContentBlock::Text { .. } => "text",
            ContentBlock::Thinking { .. } => "thinking",
            ContentBlock::RedactedThinking { .. } => "redacted_thinking",
            ContentBlock::ToolUse { .. } => "tool_use",
            ContentBlock::ToolResult { .. } => "tool_result",
            ContentBlock::Other(value) => value.get("type").and_then(serde_json::Value::as_str).unwrap_or("?"),
        }
    }

    /// Set or clear the cache breakpoint on a block that can carry one; a block of
    /// another kind is left as it is.
    fn set_cache_control(&mut self, marker: Option<CacheControl>) {
        match self {
            ContentBlock::Text { cache_control, .. } | ContentBlock::ToolResult { cache_control, .. } => *cache_control = marker,
            _ => {}
        }
    }
}

/// The typed variants, borrowed, in the API's internally tagged shape.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum KnownRef<'a> {
    Text {
        text: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<&'a CacheControl>,
    },
    Thinking {
        thinking: &'a str,
        signature: &'a str,
    },
    RedactedThinking {
        data: &'a str,
    },
    ToolUse {
        id: &'a str,
        name: &'a str,
        input: &'a serde_json::Value,
    },
    ToolResult {
        tool_use_id: &'a str,
        content: &'a str,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<&'a CacheControl>,
    },
}

/// The typed variants, owned, for deserialisation.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Known {
    Text {
        text: String,
        #[serde(default)]
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
        #[serde(default)]
        content: String,
        #[serde(default)]
        is_error: bool,
        #[serde(default)]
        cache_control: Option<CacheControl>,
    },
}

impl Serialize for ContentBlock {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            ContentBlock::Text { text, cache_control } => KnownRef::Text { text, cache_control: cache_control.as_ref() }.serialize(serializer),
            ContentBlock::Thinking { thinking, signature } => KnownRef::Thinking { thinking, signature }.serialize(serializer),
            ContentBlock::RedactedThinking { data } => KnownRef::RedactedThinking { data }.serialize(serializer),
            ContentBlock::ToolUse { id, name, input } => KnownRef::ToolUse { id, name, input }.serialize(serializer),
            ContentBlock::ToolResult { tool_use_id, content, is_error, cache_control } => {
                KnownRef::ToolResult { tool_use_id, content, is_error: *is_error, cache_control: cache_control.as_ref() }.serialize(serializer)
            }
            ContentBlock::Other(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ContentBlock {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let kind = value.get("type").and_then(serde_json::Value::as_str).ok_or_else(|| serde::de::Error::custom("a content block has no `type`"))?;
        match kind {
            "text" | "thinking" | "redacted_thinking" | "tool_use" | "tool_result" => {
                let known: Known = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(match known {
                    Known::Text { text, cache_control } => ContentBlock::Text { text, cache_control },
                    Known::Thinking { thinking, signature } => ContentBlock::Thinking { thinking, signature },
                    Known::RedactedThinking { data } => ContentBlock::RedactedThinking { data },
                    Known::ToolUse { id, name, input } => ContentBlock::ToolUse { id, name, input },
                    Known::ToolResult { tool_use_id, content, is_error, cache_control } => ContentBlock::ToolResult { tool_use_id, content, is_error, cache_control },
                })
            }
            _ => Ok(ContentBlock::Other(value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

impl Message {
    pub fn user(content: Vec<ContentBlock>) -> Self {
        Self { role: Role::User, content }
    }
    pub fn assistant(content: Vec<ContentBlock>) -> Self {
        Self { role: Role::Assistant, content }
    }
    /// The text blocks, joined.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Present on a refusal only.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemBlock {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// One assembled response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// The beta the OAuth bearer credential needs.
pub const BETA_OAUTH: &str = "oauth-2025-04-20";
/// The beta `fallbacks: "default"` needs.
pub const BETA_FALLBACK: &str = "server-side-fallback-2026-07-01";

/// The wire body of a request.
///
/// The render order is tools, system, messages — the stable prefix first — and the
/// cache breakpoints are placed here, not by the caller: on the last tool (tools sorted
/// by name), on `system[0]`, and on the last block of the last user message. A marker
/// the caller set on any other block is dropped, so a request never carries more than
/// the four breakpoints the API allows; a last user block of a kind that cannot carry a
/// marker (neither text nor a tool result) carries none. Thinking is adaptive with a
/// summarised display; `budget_tokens` is never sent. `fallbacks: "default"` is sent
/// when the request asks for it.
pub fn body(req: &Request) -> serde_json::Value {
    let mut tools = req.tools.clone();
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    for tool in &mut tools {
        tool.cache_control = None;
    }
    if let Some(last) = tools.last_mut() {
        last.cache_control = Some(CacheControl::ephemeral());
    }

    let mut system = req.system.clone();
    for (i, block) in system.iter_mut().enumerate() {
        block.cache_control = (i == 0).then(CacheControl::ephemeral);
    }

    let mut messages = req.messages.clone();
    for message in &mut messages {
        for block in &mut message.content {
            block.set_cache_control(None);
        }
    }
    if let Some(last_user) = messages.iter_mut().rev().find(|m| m.role == Role::User)
        && let Some(block) = last_user.content.last_mut()
    {
        block.set_cache_control(Some(CacheControl::ephemeral()));
    }

    let mut body = serde_json::json!({
        "model": req.model,
        "max_tokens": req.max_tokens,
        "stream": true,
        "messages": messages,
        "thinking": {"type": "adaptive", "display": "summarized"},
        "output_config": {"effort": req.effort},
    });
    if !system.is_empty() {
        body["system"] = serde_json::json!(system);
    }
    if !tools.is_empty() {
        body["tools"] = serde_json::json!(tools);
    }
    if req.fallbacks {
        body["fallbacks"] = serde_json::json!("default");
    }
    body
}

/// The betas a request needs, joined by the client into one `anthropic-beta` header.
pub fn betas(req: &Request, credential: &Credential) -> Vec<&'static str> {
    let mut betas = Vec::new();
    if matches!(credential, Credential::Bearer(_)) {
        betas.push(BETA_OAUTH);
    }
    if req.fallbacks {
        betas.push(BETA_FALLBACK);
    }
    betas
}
