//! A `satz mcp` child, spoken to over stdio with rmcp as the client. One per open
//! estate; the root is the estate's directory (satz confines every path to it), the
//! ceiling is [`Allow`]. Every rmcp type stays inside this file: the rest of the app
//! sees [`ToolInfo`] and [`ToolOutcome`].

use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rmcp::model::{CallToolRequestParams, ReadResourceRequestParams, ResourceContents, Tool};
use rmcp::service::{RoleClient, RunningService, ServiceError, ServiceExt};
use rmcp::transport::TokioChildProcess;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::broadcast;

use super::reports::OpenReport;
use super::{Allow, SatzBinary, SatzError};

/// How many stderr lines a slow reader may fall behind before it is told so, and how
/// many the backlog keeps.
const STDERR_LINES: usize = 256;
/// The last lines the child wrote to stderr, for a reader that arrives after them.
type Backlog = Arc<Mutex<VecDeque<String>>>;
/// How long the child's stderr may stay open after the child was told to exit.
const STDERR_DRAIN: Duration = Duration::from_secs(5);

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

impl From<Tool> for ToolInfo {
    fn from(t: Tool) -> Self {
        let annotations = t.annotations.map(|a| ToolAnnotations {
            read_only: a.read_only_hint,
            destructive: a.destructive_hint,
            idempotent: a.idempotent_hint,
            open_world: a.open_world_hint,
        });
        ToolInfo {
            name: t.name.into_owned(),
            description: t.description.map(|d| d.into_owned()).unwrap_or_default(),
            input_schema: serde_json::Value::Object(Arc::unwrap_or_clone(t.input_schema)),
            output_schema: t
                .output_schema
                .map(|s| serde_json::Value::Object(Arc::unwrap_or_clone(s))),
            annotations: annotations.unwrap_or_default(),
        }
    }
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
            return Err(SatzError::Refused {
                tool: tool.to_string(),
                text: self.text.clone(),
            });
        }
        let value = self.structured.clone().ok_or_else(|| {
            SatzError::Mcp(format!("{tool}: no structured content in the result"))
        })?;
        serde_json::from_value(value).map_err(|e| SatzError::Json {
            command: tool.to_string(),
            source: e,
        })
    }
}

pub struct McpSession {
    instructions: String,
    guide: String,
    tools: Vec<ToolInfo>,
    open: OpenReport,
    stderr: broadcast::Sender<String>,
    backlog: Backlog,
    service: RunningService<RoleClient, ()>,
    /// forwards the child's stderr into `stderr`; ends when the pipe closes
    pump: tokio::task::JoinHandle<()>,
    pid: Option<u32>,
}

impl std::fmt::Debug for McpSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpSession")
            .field("estate", &self.open.estate)
            .field("tools", &self.tools.len())
            .finish()
    }
}

impl McpSession {
    /// Spawn `satz mcp --root <root> --allow <allow>`, initialize, list the tools, read
    /// `satz://guide`, and open the estate (`config` and `estate` as `satz_open` takes them).
    /// A refused `satz_open` is [`SatzError::Refused`]: nothing works without an open estate.
    pub async fn open(
        bin: &SatzBinary,
        root: &Path,
        allow: Allow,
        config: &str,
        estate: &str,
    ) -> Result<McpSession, SatzError> {
        let mut cmd = tokio::process::Command::new(&bin.path);
        cmd.arg("mcp")
            .arg("--root")
            .arg(root)
            .arg("--allow")
            .arg(allow.as_arg())
            .kill_on_drop(true);
        let (transport, stderr_pipe) = TokioChildProcess::builder(cmd)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| SatzError::Io {
                context: format!(
                    "spawning `{} mcp --root {}`",
                    bin.path.display(),
                    root.display()
                ),
                source: e,
            })?;
        let stderr_pipe = stderr_pipe.expect("stderr is piped");
        let pid = transport.id();

        let (stderr, _) = broadcast::channel(STDERR_LINES);
        let backlog: Backlog = Arc::default();
        let pump = tokio::spawn(pump_stderr(
            stderr_pipe,
            stderr.clone(),
            Arc::clone(&backlog),
        ));

        let service = match ().serve(transport).await {
            Ok(s) => s,
            Err(e) => {
                // What satz said before it died is the error.
                pump.abort();
                let said = backlog
                    .lock()
                    .expect("the backlog lock is never poisoned")
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");
                return Err(SatzError::Mcp(format!("initialize failed: {e}\n{said}")));
            }
        };

        let info = service
            .peer_info()
            .ok_or_else(|| SatzError::Mcp("no server info after initialize".to_string()))?;
        let instructions = info.instructions.clone().unwrap_or_default();
        let tools = service
            .list_all_tools()
            .await
            .map_err(service_error)?
            .into_iter()
            .map(ToolInfo::from)
            .collect();
        let guide = read_text_resource(&service, "satz://guide").await?;

        let mut args = serde_json::Map::new();
        args.insert(
            "config".to_string(),
            serde_json::Value::String(config.to_string()),
        );
        args.insert(
            "estate".to_string(),
            serde_json::Value::String(estate.to_string()),
        );
        let open = call_on(&service, "satz_open", args)
            .await?
            .typed::<OpenReport>("satz_open")?;

        Ok(McpSession {
            instructions,
            guide,
            tools,
            open,
            stderr,
            backlog,
            service,
            pump,
            pid,
        })
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
    /// The child's stderr, line by line — satz's progress and notes go there. A receiver
    /// that falls more than 256 lines behind reads `Lagged` and continues; the pump
    /// never waits for it.
    pub fn stderr(&self) -> broadcast::Receiver<String> {
        self.stderr.subscribe()
    }
    /// The last 256 lines the child wrote to stderr — the banner and the level line
    /// among them — for a reader that subscribes after they were written.
    pub fn stderr_backlog(&self) -> Vec<String> {
        self.backlog
            .lock()
            .expect("the backlog lock is never poisoned")
            .iter()
            .cloned()
            .collect()
    }
    /// The child's process id, for the Commands view.
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Call a tool. A refusal is a [`ToolOutcome`] with `is_error`; a child that has
    /// exited is [`SatzError::Closed`].
    pub async fn call(
        &self,
        tool: &str,
        args: serde_json::Map<String, serde_json::Value>,
    ) -> Result<ToolOutcome, SatzError> {
        call_on(&self.service, tool, args).await
    }

    /// Cancel the service, which closes the child's stdin and waits for it to exit
    /// (rmcp kills it after three seconds), then drain the rest of its stderr.
    pub async fn close(self) -> Result<(), SatzError> {
        let McpSession { service, pump, .. } = self;
        service
            .cancel()
            .await
            .map_err(|e| SatzError::Mcp(format!("closing: {e}")))?;
        match tokio::time::timeout(STDERR_DRAIN, pump).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(SatzError::Mcp(format!(
                "closing: the stderr reader panicked: {e}"
            ))),
            Err(_) => Err(SatzError::Mcp(format!(
                "closing: the child's stderr stayed open for {STDERR_DRAIN:?} after it was told to exit"
            ))),
        }
    }
}

async fn pump_stderr(
    pipe: tokio::process::ChildStderr,
    out: broadcast::Sender<String>,
    backlog: Backlog,
) {
    let mut lines = BufReader::new(pipe).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                {
                    let mut kept = backlog.lock().expect("the backlog lock is never poisoned");
                    if kept.len() == STDERR_LINES {
                        kept.pop_front();
                    }
                    kept.push_back(line.clone());
                }
                // `send` fails only when nobody is subscribed; the backlog has the line.
                let _ = out.send(line);
            }
            Ok(None) => break,
            Err(e) => {
                tracing::warn!("satz mcp: reading the child's stderr: {e}");
                break;
            }
        }
    }
}

async fn call_on(
    service: &RunningService<RoleClient, ()>,
    tool: &str,
    args: serde_json::Map<String, serde_json::Value>,
) -> Result<ToolOutcome, SatzError> {
    let params = CallToolRequestParams::new(tool.to_string()).with_arguments(args);
    let result = service.call_tool(params).await.map_err(service_error)?;
    let text = result
        .content
        .iter()
        .filter_map(|c| c.as_text())
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(ToolOutcome {
        structured: result.structured_content,
        text,
        is_error: result.is_error.unwrap_or(false),
    })
}

async fn read_text_resource(
    service: &RunningService<RoleClient, ()>,
    uri: &str,
) -> Result<String, SatzError> {
    let result = service
        .read_resource(ReadResourceRequestParams::new(uri))
        .await
        .map_err(service_error)?;
    let mut text = String::new();
    for contents in result.contents {
        match contents {
            ResourceContents::TextResourceContents { text: t, .. } => text.push_str(&t),
            _ => return Err(SatzError::Mcp(format!("{uri}: not a text resource"))),
        }
    }
    Ok(text)
}

/// A transport that is gone — closed, or a pipe the child no longer reads — is
/// [`SatzError::Closed`]; everything else is [`SatzError::Mcp`].
fn service_error(e: ServiceError) -> SatzError {
    match e {
        ServiceError::TransportClosed => SatzError::Closed("transport closed".to_string()),
        ServiceError::TransportSend(e) => SatzError::Closed(format!("send failed: {e}")),
        ServiceError::Cancelled { reason } => {
            SatzError::Closed(reason.unwrap_or_else(|| "cancelled".to_string()))
        }
        other => SatzError::Mcp(other.to_string()),
    }
}
