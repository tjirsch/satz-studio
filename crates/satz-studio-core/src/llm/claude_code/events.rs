//! The lines Claude Code writes on stdout in `--output-format stream-json`, typed.
//!
//! One JSON object per line. The kinds this app reads are variants; every other kind
//! — the status and thinking-token notes, the retry note — is [`CcLine::Other`] and is
//! ignored. A line that is not JSON at all is an error, never skipped: the session
//! would otherwise carry on against a CLI it no longer understands.

use serde::Deserialize;

use crate::llm::Usage;

/// One line of Claude Code's output stream.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CcLine {
    /// `system` with its `subtype`: `init` once per session, then `status`,
    /// `thinking_tokens`, `api_retry` and `permission_denied`.
    System {
        subtype: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        mcp_servers: Vec<McpServer>,
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        tool_name: Option<String>,
    },
    /// one Anthropic Messages API stream event, verbatim
    StreamEvent { event: serde_json::Value },
    /// the assistant message each block set completes into; the stream already
    /// carried its content, so only its usage is read
    Assistant {
        #[serde(default)]
        message: AssistantMessage,
    },
    /// a user message Claude Code made: the results of the tools it ran itself
    User { message: UserMessage },
    /// the subscription's usage against the plan's windows
    RateLimitEvent { rate_limit_info: RateLimit },
    /// the CLI asks the client something — a tool that needs approval
    ControlRequest {
        request_id: String,
        request: ControlRequest,
    },
    /// the CLI's answer to a request the client made
    ControlResponse { response: ControlResponse },
    /// a pending request the CLI withdrew
    ControlCancelRequest { request_id: String },
    /// the turn ended
    #[serde(rename = "result")]
    Ended {
        subtype: String,
        #[serde(default)]
        is_error: bool,
        #[serde(default)]
        usage: Usage,
        #[serde(default)]
        result: Option<String>,
        #[serde(default)]
        session_id: Option<String>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpServer {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AssistantMessage {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Usage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserMessage {
    #[serde(default)]
    pub content: UserContent,
}

/// A user message's content: the text of a message the operator sent back, or the
/// blocks Claude Code made — which is how a tool result arrives.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum UserContent {
    Text(String),
    Blocks(Vec<serde_json::Value>),
    /// a shape this app does not read
    Other(serde_json::Value),
}

impl Default for UserContent {
    fn default() -> Self {
        UserContent::Other(serde_json::Value::Null)
    }
}

/// One `tool_result` block out of a user message.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResultBlock {
    pub tool_use_id: String,
    /// the content as text: a string verbatim, a block list's text joined, anything
    /// else pretty-printed
    pub text: String,
    /// the content parsed as JSON when it is JSON
    pub structured: Option<serde_json::Value>,
    pub is_error: bool,
}

impl UserMessage {
    /// Every `tool_result` block the message carries, in order.
    pub fn tool_results(&self) -> Vec<ToolResultBlock> {
        let UserContent::Blocks(blocks) = &self.content else {
            return Vec::new();
        };
        blocks
            .iter()
            .filter(|b| b.get("type").and_then(serde_json::Value::as_str) == Some("tool_result"))
            .filter_map(|b| {
                let tool_use_id = b.get("tool_use_id")?.as_str()?.to_string();
                let text = content_text(b.get("content"));
                let structured = serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .filter(|v| v.is_object() || v.is_array());
                Some(ToolResultBlock {
                    tool_use_id,
                    text,
                    structured,
                    is_error: b
                        .get("is_error")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                })
            })
            .collect()
    }
}

/// A tool result's content: a string verbatim, a list of content blocks with their
/// `text` joined, anything else pretty-printed.
fn content_text(value: Option<&serde_json::Value>) -> String {
    match value {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(blocks)) => blocks
            .iter()
            .map(
                |b| match b.get("text").and_then(serde_json::Value::as_str) {
                    Some(text) => text.to_string(),
                    None => b.to_string(),
                },
            )
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    }
}

/// What the CLI asks the client.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum ControlRequest {
    CanUseTool {
        tool_name: String,
        #[serde(default)]
        input: serde_json::Value,
        #[serde(default)]
        tool_use_id: Option<String>,
    },
    #[serde(other)]
    Other,
}

/// The CLI's answer to a request the client made. The success payload is not read:
/// the initialize answer carries the account, and the app keeps none of it.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum ControlResponse {
    Success {
        request_id: String,
    },
    Error {
        #[serde(default)]
        request_id: Option<String>,
        error: String,
    },
}

/// The plan's usage, as the CLI reports it between requests.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RateLimit {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(rename = "rateLimitType", default)]
    pub window: Option<String>,
    /// 0.0–1.0
    #[serde(default)]
    pub utilization: Option<f64>,
    /// unix seconds
    #[serde(rename = "resetsAt", default)]
    pub resets_at: Option<i64>,
}

impl RateLimit {
    /// The line the Chat footer shows: which window, how much of it is used, and when
    /// it resets. `now` is unix seconds.
    pub fn notice(&self, now: i64) -> String {
        let window = match self.window.as_deref() {
            Some("seven_day") => "seven-day".to_string(),
            Some("five_hour") => "five-hour".to_string(),
            Some(other) => other.replace('_', "-"),
            None => "plan".to_string(),
        };
        let used = match self.utilization {
            Some(u) => format!("{:.0}% of the {window} limit used", u * 100.0),
            None => format!("the {window} limit"),
        };
        match self.resets_at.map(|at| at - now) {
            Some(left) if left > 0 => format!("Claude plan: {used}, resets in {}", duration(left)),
            _ => format!("Claude plan: {used}"),
        }
    }
}

/// `6 d 2 h`, `2 h 5 min`, `40 min` — the two largest units that are not zero.
fn duration(seconds: i64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        return format!("{days} d {hours} h");
    }
    if hours > 0 {
        return format!("{hours} h {minutes} min");
    }
    format!("{} min", minutes.max(1))
}

/// Unix seconds now.
pub fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(json: &str) -> CcLine {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn the_init_line_carries_the_session_the_model_and_the_servers() {
        let CcLine::System {
            subtype,
            session_id,
            model,
            mcp_servers,
            ..
        } = line(
            r#"{"type":"system","subtype":"init","session_id":"s1","model":"claude-opus-5","permissionMode":"default","mcp_servers":[{"name":"satz","status":"connected"}],"tools":["mcp__satz__satz_questions"]}"#,
        )
        else {
            panic!("a system line");
        };
        assert_eq!(subtype, "init");
        assert_eq!(session_id.as_deref(), Some("s1"));
        assert_eq!(model.as_deref(), Some("claude-opus-5"));
        assert_eq!(mcp_servers.len(), 1);
        assert_eq!(mcp_servers[0].status, "connected");
    }

    #[test]
    fn a_line_this_code_does_not_read_is_other_and_not_an_error() {
        assert!(matches!(
            line(r#"{"type":"prompt_suggestion","text":"next"}"#),
            CcLine::Other
        ));
        assert!(matches!(
            line(r#"{"type":"system","subtype":"thinking_tokens","estimated_tokens":50}"#),
            CcLine::System { .. }
        ));
    }

    #[test]
    fn a_tool_result_is_read_as_text_and_as_json_when_it_is_json() {
        let CcLine::User { message } = line(
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"{\"estates\":[]}","is_error":false}]}}"#,
        ) else {
            panic!("a user line");
        };
        let results = message.tool_results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_use_id, "t1");
        assert_eq!(
            results[0].structured,
            Some(serde_json::json!({"estates": []}))
        );
        assert!(!results[0].is_error);

        let CcLine::User { message } = line(
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t2","content":[{"type":"text","text":"refused"}],"is_error":true}]}}"#,
        ) else {
            panic!("a user line");
        };
        let results = message.tool_results();
        assert_eq!(results[0].text, "refused");
        assert_eq!(results[0].structured, None);
        assert!(results[0].is_error);
    }

    #[test]
    fn a_user_message_of_plain_text_carries_no_tool_result() {
        let CcLine::User { message } =
            line(r#"{"type":"user","message":{"role":"user","content":"hello"}}"#)
        else {
            panic!("a user line");
        };
        assert!(message.tool_results().is_empty());
    }

    #[test]
    fn a_can_use_tool_request_carries_the_tool_the_input_and_the_call_id() {
        let CcLine::ControlRequest {
            request_id,
            request:
                ControlRequest::CanUseTool {
                    tool_name,
                    input,
                    tool_use_id,
                },
        } = line(
            r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"mcp__satz__satz_interview","input":{"answers":{"x":true}},"tool_use_id":"toolu_1","permission_suggestions":[]}}"#,
        )
        else {
            panic!("a can_use_tool request");
        };
        assert_eq!(request_id, "r1");
        assert_eq!(tool_name, "mcp__satz__satz_interview");
        assert_eq!(tool_use_id.as_deref(), Some("toolu_1"));
        assert_eq!(input, serde_json::json!({"answers": {"x": true}}));
    }

    #[test]
    fn the_result_line_carries_the_end_and_the_usage() {
        let CcLine::Ended {
            subtype,
            is_error,
            usage,
            result,
            ..
        } = line(
            r#"{"type":"result","subtype":"success","is_error":false,"num_turns":2,"usage":{"input_tokens":18,"output_tokens":230,"cache_read_input_tokens":13131,"cache_creation_input_tokens":642},"result":"done","session_id":"s1"}"#,
        )
        else {
            panic!("a result line");
        };
        assert_eq!(subtype, "success");
        assert!(!is_error);
        assert_eq!(usage.input_tokens, 18);
        assert_eq!(usage.cache_read_input_tokens, Some(13131));
        assert_eq!(result.as_deref(), Some("done"));
    }

    #[test]
    fn the_rate_limit_reads_as_a_line_for_the_footer() {
        let CcLine::RateLimitEvent { rate_limit_info } = line(
            r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","resetsAt":1000000,"rateLimitType":"seven_day","utilization":0.54}}"#,
        ) else {
            panic!("a rate limit line");
        };
        assert_eq!(
            rate_limit_info.notice(1000000 - 194_400),
            "Claude plan: 54% of the seven-day limit used, resets in 2 d 6 h"
        );
        assert_eq!(
            rate_limit_info.notice(1000000),
            "Claude plan: 54% of the seven-day limit used"
        );
    }

    #[test]
    fn a_control_response_says_which_request_it_answers() {
        assert!(matches!(
            line(r#"{"type":"control_response","response":{"subtype":"success","request_id":"init-1","response":{"commands":[]}}}"#),
            CcLine::ControlResponse { response: ControlResponse::Success { request_id } } if request_id == "init-1"
        ));
        assert!(matches!(
            line(r#"{"type":"control_response","response":{"subtype":"error","request_id":"init-1","error":"no"}}"#),
            CcLine::ControlResponse { response: ControlResponse::Error { error, .. } } if error == "no"
        ));
    }

    #[test]
    fn the_reset_distance_reads_in_the_two_largest_units() {
        assert_eq!(duration(2 * 86_400 + 6 * 3_600), "2 d 6 h");
        assert_eq!(duration(2 * 3_600 + 300), "2 h 5 min");
        assert_eq!(duration(2_400), "40 min");
        assert_eq!(duration(5), "1 min");
    }
}
