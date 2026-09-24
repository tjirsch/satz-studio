//! A `satz mcp` child, spoken to over stdio with rmcp as the client. One per open
//! estate; the root is the one `satz mcp-config` names for the estate (satz confines
//! every path argument to it), the ceiling is [`Allow`]. Every rmcp type stays inside
//! this file: the rest of the app sees [`ToolOutcome`].

use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rmcp::model::{CallToolRequestParams, ErrorCode};
use rmcp::service::{RoleClient, RunningService, ServiceError, ServiceExt};
use rmcp::transport::TokioChildProcess;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::reports::OpenReport;
use super::{Allow, SatzBinary, SatzError};

/// How many stderr lines the backlog keeps.
const STDERR_LINES: usize = 256;
/// The last lines the child wrote to stderr: what an initialize that failed reports.
type Backlog = Arc<Mutex<VecDeque<String>>>;
/// How long the child's stderr may stay open after the child was told to exit.
const STDERR_DRAIN: Duration = Duration::from_secs(5);

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
    open: OpenReport,
    service: RunningService<RoleClient, ()>,
    /// reads the child's stderr into the backlog; ends when the pipe closes
    pump: tokio::task::JoinHandle<()>,
    pid: Option<u32>,
}

impl std::fmt::Debug for McpSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpSession")
            .field("estate", &self.open.estate)
            .finish()
    }
}

impl McpSession {
    /// Spawn `satz mcp --root <root> --allow <allow>`, initialize, and open the estate
    /// (`config` and `estate` as `satz_open` takes them).
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

        let backlog: Backlog = Arc::default();
        let pump = tokio::spawn(pump_stderr(stderr_pipe, Arc::clone(&backlog)));

        let service = match ().serve(transport).await {
            Ok(s) => s,
            Err(e) => {
                // What satz said before it died is the error: its stderr closes when it
                // exits, so the pump is awaited (bounded) before the backlog is read.
                let _ = tokio::time::timeout(STDERR_DRAIN, pump).await;
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
            open,
            service,
            pump,
            pid,
        })
    }

    pub fn open_report(&self) -> &OpenReport {
        &self.open
    }
    /// The child's process id.
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Call a tool. A refusal is a [`ToolOutcome`] with `is_error`; a JSON-RPC
    /// `invalid_params` error in place of a result is [`SatzError::InvalidParams`]; a
    /// child that has exited is [`SatzError::Closed`].
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

/// Read the child's stderr to its end, keeping the last lines. It is read whether or
/// not anybody asks, because a child whose stderr pipe fills blocks on its next write.
async fn pump_stderr(pipe: tokio::process::ChildStderr, backlog: Backlog) {
    let mut lines = BufReader::new(pipe).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let mut kept = backlog.lock().expect("the backlog lock is never poisoned");
                if kept.len() == STDERR_LINES {
                    kept.pop_front();
                }
                kept.push_back(line);
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
    let result = service.call_tool(params).await.map_err(|e| match e {
        ServiceError::McpError(e) if e.code == ErrorCode::INVALID_PARAMS => {
            SatzError::InvalidParams {
                tool: tool.to_string(),
                message: match e.data {
                    Some(data) => format!("{} ({data})", e.message),
                    None => e.message.into_owned(),
                },
            }
        }
        other => service_error(other),
    })?;
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
