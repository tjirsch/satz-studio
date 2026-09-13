//! A `satz mcp` child, spoken to over stdio with rmcp as the client. One per open
//! estate; the root is the estate's directory (satz confines every path to it), the
//! ceiling is [`Allow`]. Every rmcp type stays inside this file: the rest of the app
//! sees [`ToolInfo`] and [`ToolOutcome`].

use std::path::Path;

use tokio::sync::broadcast;

use super::reports::OpenReport;
use super::{Allow, SatzBinary, SatzError};

/// The annotations satz puts on a tool — the approval gate reads them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ToolAnnotations {
    pub read_only: Option<bool>,
    pub destructive: Option<bool>,
    pub idempotent: Option<bool>,
    pub open_world: Option<bool>,
}

impl ToolAnnotations {
    /// Runs without asking: satz says it reads only.
    pub fn is_read_only(&self) -> bool {
        self.read_only == Some(true)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    /// a JSON Schema object, verbatim — what Claude receives as `input_schema`
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    pub annotations: ToolAnnotations,
}

/// What a tool call returned. A refusal is `is_error` — a result the caller can act on,
/// never a transport error.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolOutcome {
    pub structured: Option<serde_json::Value>,
    pub text: String,
    pub is_error: bool,
}

impl ToolOutcome {
    /// The structured payload, typed; a refusal or a payload of another shape is an error.
    pub fn typed<T: serde::de::DeserializeOwned>(&self, tool: &str) -> Result<T, SatzError> {
        if self.is_error {
            return Err(SatzError::Refused { tool: tool.to_string(), text: self.text.clone() });
        }
        let value = self.structured.clone().ok_or_else(|| SatzError::Mcp(format!("{tool}: no structured content in the result")))?;
        serde_json::from_value(value).map_err(|e| SatzError::Json { command: tool.to_string(), source: e })
    }
}

pub struct McpSession {
    instructions: String,
    guide: String,
    tools: Vec<ToolInfo>,
    open: OpenReport,
    stderr: broadcast::Sender<String>,
}

impl std::fmt::Debug for McpSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpSession").field("estate", &self.open.estate).field("tools", &self.tools.len()).finish()
    }
}

impl McpSession {
    /// Spawn `satz mcp --root <root> --allow <allow>`, initialize, list the tools, read
    /// `satz://guide`, and open the estate (`config` and `estate` as `satz_open` takes them).
    pub async fn open(bin: &SatzBinary, root: &Path, allow: Allow, config: &str, estate: &str) -> Result<McpSession, SatzError> {
        let _ = (bin, root, allow, config, estate);
        Err(crate::Unimplemented::new("McpSession::open", "U4").into())
    }

    /// The `instructions` the server returned at initialize: the CLI-to-tool map, the
    /// refusal list and the capability level in force.
    pub fn instructions(&self) -> &str {
        &self.instructions
    }
    /// The text of `satz://guide` — what an agent reads before writing a `.satz` file.
    pub fn guide(&self) -> &str {
        &self.guide
    }
    pub fn tools(&self) -> &[ToolInfo] {
        &self.tools
    }
    pub fn tool(&self, name: &str) -> Option<&ToolInfo> {
        self.tools.iter().find(|t| t.name == name)
    }
    pub fn open_report(&self) -> &OpenReport {
        &self.open
    }
    /// The child's stderr, line by line — satz's progress and notes go there.
    pub fn stderr(&self) -> broadcast::Receiver<String> {
        self.stderr.subscribe()
    }

    pub async fn call(&self, tool: &str, args: serde_json::Map<String, serde_json::Value>) -> Result<ToolOutcome, SatzError> {
        let _ = (tool, args);
        Err(crate::Unimplemented::new("McpSession::call", "U4").into())
    }

    pub async fn close(self) -> Result<(), SatzError> {
        Err(crate::Unimplemented::new("McpSession::close", "U4").into())
    }
}
