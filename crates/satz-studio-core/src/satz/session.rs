//! One session per open estate: the CLI runner and the MCP child, the write lock every
//! writer takes, and the identity the estate's live tools run as — read from
//! `satz_open`, displayed, configured nowhere. The Commands view and the agent's tool
//! bridge both go through [`EstateSession::tool`], so one identity per estate holds.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::{Mutex, MutexGuard};

use super::{Allow, McpSession, SatzBinary, SatzCli, SatzError, ToolInfo, ToolOutcome};
use crate::estate::EstateDir;

pub struct EstateSession {
    pub dir: EstateDir,
    /// the main `.satz` file, absolute
    pub main: PathBuf,
    pub cli: SatzCli,
    mcp: McpSession,
    write_lock: Mutex<()>,
}

impl std::fmt::Debug for EstateSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EstateSession").field("main", &self.main).field("runs_as", &self.runs_as()).finish()
    }
}

impl EstateSession {
    pub async fn open(bin: &SatzBinary, dir: EstateDir, main: PathBuf, allow: Allow) -> Result<Arc<EstateSession>, SatzError> {
        let _ = (bin, dir, main, allow);
        Err(crate::Unimplemented::new("EstateSession::open", "U4").into())
    }

    /// The identity this estate's live tools run as, as `satz_open` reported it.
    pub fn runs_as(&self) -> Option<&str> {
        self.mcp.open_report().runs_as.as_deref()
    }
    pub fn deployment_mode(&self) -> Option<&str> {
        self.mcp.open_report().deployment_mode.as_deref()
    }
    pub fn tools(&self) -> &[ToolInfo] {
        self.mcp.tools()
    }
    pub fn tool_info(&self, name: &str) -> Option<&ToolInfo> {
        self.mcp.tool(name)
    }
    pub fn instructions(&self) -> &str {
        self.mcp.instructions()
    }
    pub fn guide(&self) -> &str {
        self.mcp.guide()
    }

    /// Call a tool on this estate's session.
    pub async fn tool(&self, name: &str, args: serde_json::Map<String, serde_json::Value>) -> Result<ToolOutcome, SatzError> {
        self.mcp.call(name, args).await
    }

    /// Every writer takes this first: a view edit, an interview answer, an agent's
    /// write tool. Held across the check and the rename.
    pub async fn write_lock(&self) -> MutexGuard<'_, ()> {
        self.write_lock.lock().await
    }

    /// `apply` and `bootstrap` run in the user's own terminal: a one-shot script under
    /// the app's data directory (`cd <estate> && satz --config . <args…>`), opened with
    /// the OS terminal. Returns the script's path.
    pub fn external_command(&self, args: &[String]) -> Result<PathBuf, SatzError> {
        let _ = args;
        Err(crate::Unimplemented::new("EstateSession::external_command", "U4").into())
    }

    /// Open a script in the OS terminal (macOS `open -a Terminal`, Linux
    /// `x-terminal-emulator`, Windows `wt` or `cmd /k`).
    pub fn open_in_terminal(script: &Path) -> Result<(), SatzError> {
        let _ = script;
        Err(crate::Unimplemented::new("EstateSession::open_in_terminal", "U4").into())
    }
}
