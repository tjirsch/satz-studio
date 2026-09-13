//! The one error type of the Claude client, the agent loop and the provider adapters,
//! and the mapping from an HTTP status to it.

use std::time::Duration;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ClaudeError {
    #[error("bad request: {message}{}", request_id.as_deref().map(|id| format!(" (request-id {id})")).unwrap_or_default())]
    BadRequest {
        message: String,
        request_id: Option<String>,
    },
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("permission denied: {0}")]
    Permission(String),
    #[error("model not found: {0}")]
    NotFound(String),
    #[error("request too large")]
    RequestTooLarge,
    #[error("rate limited{}", retry_after.map(|d| format!(", retry after {}s", d.as_secs())).unwrap_or_default())]
    RateLimited { retry_after: Option<Duration> },
    #[error("the API is overloaded")]
    Overloaded,
    #[error("server error {status}")]
    Server { status: u16 },
    /// A status none of the variants above names.
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("connection: {0}")]
    Connection(String),
    #[error("stream: {0}")]
    Stream(String),
    #[error("refused{}{}", category.as_deref().map(|c| format!(" ({c})")).unwrap_or_default(), explanation.as_deref().map(|e| format!(": {e}")).unwrap_or_default())]
    Refused {
        category: Option<String>,
        explanation: Option<String>,
        recommended_model: Option<String>,
    },
    /// Nothing resolved; `tried` says what each source answered.
    #[error("no credential: set ANTHROPIC_API_KEY, or ANTHROPIC_AUTH_TOKEN, or log in with `ant auth login`, or enter a key in Settings{}", tried.iter().map(|t| format!("\n  {t}")).collect::<String>())]
    NoCredential { tried: Vec<String> },
    #[error("keychain: {0}")]
    Keychain(String),
    /// A tool call failed below the tool — the session, not the tool's own refusal.
    #[error("tool {name}: {message}")]
    Tool { name: String, message: String },
    #[error("cancelled")]
    Cancelled,
}

impl ClaudeError {
    /// The failures a request is retried on — before its first stream byte only.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ClaudeError::RateLimited { .. }
                | ClaudeError::Overloaded
                | ClaudeError::Server { .. }
                | ClaudeError::Connection(_)
        )
    }
}

/// The error for a non-2xx status. `body` is the response body: the message is its
/// `error.message` when it is the API's JSON error, the trimmed text otherwise.
pub(crate) fn from_status(
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
    body: &str,
) -> ClaudeError {
    let message = error_message(body);
    match status.as_u16() {
        400 => ClaudeError::BadRequest {
            message,
            request_id: header(headers, "request-id"),
        },
        401 => ClaudeError::Auth(message),
        403 => ClaudeError::Permission(message),
        404 => ClaudeError::NotFound(message),
        413 => ClaudeError::RequestTooLarge,
        429 => ClaudeError::RateLimited {
            retry_after: header(headers, "retry-after")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs),
        },
        529 => ClaudeError::Overloaded,
        s @ 500..=599 => ClaudeError::Server { status: s },
        s => ClaudeError::Http { status: s, message },
    }
}

fn header(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// `error.message` of the API's JSON error body, `error` when that is a string (Ollama),
/// else the body itself, trimmed.
pub(crate) fn error_message(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(message) = value
            .pointer("/error/message")
            .and_then(serde_json::Value::as_str)
        {
            return message.to_string();
        }
        if let Some(message) = value.get("error").and_then(serde_json::Value::as_str) {
            return message.to_string();
        }
    }
    body.trim().to_string()
}
