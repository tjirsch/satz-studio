//! One Claude Code process per open estate, driven over stdio.
//!
//! `claude -p --input-format stream-json --output-format stream-json` is a long-lived
//! child: the client writes one JSON object per line on its stdin, the CLI writes one
//! per line on its stdout. The loop, the context window and the tools are Claude
//! Code's; what stays the app's is the estate — the satz MCP server the CLI is given is
//! the estate's, every tool call that is not pre-approved comes back as a
//! `can_use_tool` request the app answers with its own approval card, and the estate's
//! write lock is held for the whole turn.
//!
//! The stream the CLI forwards is the Messages API's own, so [`Assembler`] folds it —
//! the same decoder the API backend uses. A turn ends on the CLI's `result` line. When
//! [`SessionOptions::log`] names a directory, every line either side writes goes to a
//! [`StreamLog`] first, before anything reads it.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::cli::{ClaudeCodeCli, ClaudeCodeError};
use super::events::{CcLine, ControlRequest, ControlResponse, RateLimit, now_seconds};
use super::log::{Channel, StreamLog, StreamLogConfig};
use crate::llm::agent::PREAMBLE;
use crate::llm::claude::sse::{Assembler, SseEvent};
use crate::llm::{AgentEvent, Approval, ClaudeError, StopReason, StreamEvent, Usage};
use crate::satz::{Allow, EstateSession, ToolInfo, ToolOutcome};

/// The MCP server the app gives Claude Code, and the prefix every one of its tools
/// carries in Claude Code's tool namespace.
pub const SERVER: &str = "satz";
/// `mcp__<server>__`
pub const TOOL_PREFIX: &str = "mcp__satz__";

/// How long the CLI has to answer the initialize request before the spawn fails.
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(30);
/// How long the CLI has to end the turn after an interrupt.
const INTERRUPT_TIMEOUT: Duration = Duration::from_secs(30);
/// How many stderr lines are kept for an error message.
const STDERR_LINES: usize = 64;

/// What a session is started with. `satz_binary` and `allow` are the app's own — the
/// satz the CLI's MCP server runs, and the ceiling it runs under.
#[derive(Debug, Clone)]
pub struct SessionOptions {
    pub model: Option<String>,
    pub max_turns: Option<u32>,
    /// a Claude Code session id to continue
    pub resume: Option<String>,
    /// pre-approve the non-destructive write tools, so they need no card
    pub auto_approve_writes: bool,
    pub satz_binary: PathBuf,
    pub allow: Allow,
    /// where the stream log goes; `None` keeps none
    pub log: Option<StreamLogConfig>,
}

/// The `--allowedTools` list: every read-only satz tool, plus the non-destructive
/// write tools when they are pre-approved. Sorted, so the command line is stable.
pub fn allowed_tools(tools: &[ToolInfo], auto_approve_writes: bool) -> Vec<String> {
    let mut names: Vec<String> = tools
        .iter()
        .filter(|t| {
            t.annotations.is_read_only()
                || (auto_approve_writes && t.annotations.destructive != Some(true))
        })
        .map(|t| format!("{TOOL_PREFIX}{}", t.name))
        .collect();
    names.sort();
    names
}

/// The `--mcp-config` payload: the estate's own satz MCP server, under the name whose
/// prefix every tool carries.
pub fn mcp_config(estate: &EstateSession, opts: &SessionOptions) -> String {
    let root = &estate.root;
    serde_json::json!({
        "mcpServers": {
            SERVER: {
                "command": opts.satz_binary.display().to_string(),
                "args": [
                    "mcp",
                    "--root",
                    root.display().to_string(),
                    "--allow",
                    opts.allow.as_arg(),
                ],
            }
        }
    })
    .to_string()
}

/// The system prompt appended to Claude Code's own: the studio preamble the API
/// backend also sends, what the satz MCP server said at initialize, the working guide,
/// and which estate this session is about — Claude Code's satz server is its own
/// process and has no estate open until the model opens one.
pub fn system_prompt(estate: &EstateSession) -> String {
    let mut parts = vec![PREAMBLE.to_string()];
    for part in [estate.instructions(), estate.guide()] {
        let part = part.trim();
        if !part.is_empty() {
            parts.push(part.to_string());
        }
    }
    parts.push(format!(
        "This session is about one estate: `{}`, configured by `{}`. \
         Call `satz_open` with that config and that estate before any other satz tool, and work on that estate alone.",
        estate.main.display(),
        estate.dir.config_path.display()
    ));
    parts.join("\n\n")
}

/// The command line the session is spawned with. `--tools \"\"` leaves Claude Code
/// without a built-in tool, so the satz server is all it can call; `--setting-sources
/// \"\"` keeps the user's own Claude Code settings out of the app's session, and
/// `--strict-mcp-config` keeps their MCP servers out; `--permission-prompt-tool stdio`
/// is what routes a tool that is not pre-approved to the app instead of denying it.
pub fn command_args(
    estate: &EstateSession,
    opts: &SessionOptions,
    system_prompt: &str,
) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--setting-sources",
        "",
        "--strict-mcp-config",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    args.push("--mcp-config".to_string());
    args.push(mcp_config(estate, opts));
    args.push("--tools".to_string());
    args.push(String::new());
    args.push("--permission-mode".to_string());
    args.push("default".to_string());
    args.push("--permission-prompt-tool".to_string());
    args.push("stdio".to_string());
    let allowed = allowed_tools(estate.tools(), opts.auto_approve_writes);
    if !allowed.is_empty() {
        args.push("--allowedTools".to_string());
        args.push(allowed.join(","));
    }
    args.push("--append-system-prompt".to_string());
    args.push(system_prompt.to_string());
    if let Some(model) = &opts.model {
        args.push("--model".to_string());
        args.push(model.clone());
    }
    if let Some(turns) = opts.max_turns {
        args.push("--max-turns".to_string());
        args.push(turns.to_string());
    }
    if let Some(resume) = &opts.resume {
        args.push("--resume".to_string());
        args.push(resume.clone());
    }
    args
}

/// A Claude Code process serving one estate.
pub struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    stderr: Arc<Mutex<Vec<String>>>,
    /// every line of the session, verbatim, when the log is on
    log: Option<Arc<StreamLog>>,
    estate: Arc<EstateSession>,
    /// the command line, for the Settings view and the tests
    args: Vec<String>,
    /// Claude Code's own session id, from `system/init`; `--resume` takes it
    session_id: Option<String>,
    /// the model `system/init` named
    model: Option<String>,
    /// the satz servers were checked once, on the first `system/init`
    checked: bool,
    /// tools the operator allowed for the rest of the session
    allowed: BTreeSet<String>,
    /// how many control requests this client has sent
    requests: u64,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeCodeSession")
            .field("estate", &self.estate.main)
            .field("session_id", &self.session_id)
            .field("model", &self.model)
            .finish()
    }
}

impl Session {
    /// Spawn the CLI in the estate's directory and initialize the control protocol.
    /// The initialize is what makes the CLI take the client as its permission handler;
    /// its answer carries the account, which this app reads nothing out of.
    pub async fn spawn(
        cli: &ClaudeCodeCli,
        estate: Arc<EstateSession>,
        opts: SessionOptions,
    ) -> Result<Session, ClaudeCodeError> {
        let args = command_args(&estate, &opts, &system_prompt(&estate));
        let log = match &opts.log {
            Some(config) => Some(Arc::new(StreamLog::create(
                config,
                &log_header(cli, &estate, &args),
            )?)),
            None => None,
        };
        let spawned = tokio::process::Command::new(&cli.path)
            .args(&args)
            .current_dir(&estate.dir.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| ClaudeCodeError::Io {
                context: format!(
                    "spawning `{} -p` in {}",
                    cli.path.display(),
                    estate.dir.dir.display()
                ),
                source: e,
            });
        let mut child = match spawned {
            Ok(child) => child,
            Err(e) => return Err(noted(log.as_deref(), "the session did not start", e)),
        };
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = BufReader::new(child.stdout.take().expect("stdout is piped")).lines();
        let stderr_pipe = child.stderr.take().expect("stderr is piped");
        let stderr: Arc<Mutex<Vec<String>>> = Arc::default();
        let sink = Arc::clone(&stderr);
        let stderr_log = log.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr_pipe).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(log) = &stderr_log {
                    // a failed write is kept by the log and returned to the session's
                    // next record, which fails the turn; this task has no turn to fail
                    let _ = log.record(Channel::Stderr, &line);
                }
                let mut held = sink.lock().expect("the stderr lock is never poisoned");
                if held.len() == STDERR_LINES {
                    held.remove(0);
                }
                held.push(line);
            }
        });

        let mut session = Session {
            child,
            stdin,
            stdout,
            stderr,
            log,
            estate,
            args,
            session_id: opts.resume.clone(),
            model: opts.model.clone(),
            checked: false,
            allowed: BTreeSet::new(),
            requests: 0,
        };
        if let Err(e) = session.initialize().await {
            return Err(noted(
                session.log.as_deref(),
                "the session did not start",
                e,
            ));
        }
        Ok(session)
    }

    /// The command line this session was spawned with.
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// The stream log this session writes, when the log is on.
    pub fn log_path(&self) -> Option<&std::path::Path> {
        self.log.as_deref().map(StreamLog::path)
    }

    /// Claude Code's session id, once it has named one.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// The model Claude Code reported for this session.
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn estate(&self) -> &Arc<EstateSession> {
        &self.estate
    }

    /// Write the initialize request and read until its answer. Nothing else arrives
    /// before the first user message, so `system/init` is read at the first turn.
    async fn initialize(&mut self) -> Result<(), ClaudeCodeError> {
        let id = self.next_request_id();
        self.write(&serde_json::json!({
            "type": "control_request",
            "request_id": id,
            "request": {"subtype": "initialize", "hooks": {}},
        }))
        .await?;
        let answered = tokio::time::timeout(INITIALIZE_TIMEOUT, async {
            loop {
                let line = self.read_line().await?;
                self.record(Channel::Stdout, &line)?;
                match parse(&line)? {
                    CcLine::ControlResponse {
                        response: ControlResponse::Success { request_id },
                    } if request_id == id => return Ok(()),
                    CcLine::ControlResponse {
                        response: ControlResponse::Error { error, .. },
                    } => {
                        return Err(ClaudeCodeError::Protocol(format!(
                            "the CLI refused the initialize request: {error}"
                        )));
                    }
                    _ => continue,
                }
            }
        })
        .await;
        match answered {
            Ok(result) => result,
            Err(_) => Err(ClaudeCodeError::Protocol(format!(
                "the CLI did not answer the initialize request within {}s{}",
                INITIALIZE_TIMEOUT.as_secs(),
                self.said()
            ))),
        }
    }

    /// One turn: the estate's write lock for its whole length, the user message, then
    /// Claude Code's stream folded into the same [`AgentEvent`]s the API backend
    /// raises. Returns when the CLI's `result` line arrives, the operator cancels, or
    /// the session breaks.
    pub async fn run_turn(
        &mut self,
        text: String,
        events: mpsc::Sender<AgentEvent>,
        cancel: CancellationToken,
    ) -> Result<(), ClaudeCodeError> {
        // Claude Code's own satz server writes this estate: the lock the app's writers
        // take is held for the turn, not per call, because the app cannot see the calls
        // it pre-approved.
        let estate = Arc::clone(&self.estate);
        let _guard = estate.write_lock().await;
        match self.turn(text, &events, &cancel).await {
            Ok(()) => Ok(()),
            Err(e) => {
                let e = noted(self.log.as_deref(), "the turn ended", e);
                // the event carries the error as it is: it already names Claude Code
                // where its variant does, and a second wrapping would say it twice
                let event = match ClaudeError::from(&e) {
                    ClaudeError::Cancelled => AgentEvent::Cancelled,
                    failed => AgentEvent::Failed(failed),
                };
                let _ = events.send(event).await;
                Err(e)
            }
        }
    }

    async fn turn(
        &mut self,
        text: String,
        events: &mpsc::Sender<AgentEvent>,
        cancel: &CancellationToken,
    ) -> Result<(), ClaudeCodeError> {
        self.write(&serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": text},
        }))
        .await?;

        let mut fold = Fold::default();
        let mut interrupt_pending = false;
        let mut interrupted = false;
        loop {
            if interrupt_pending {
                interrupt_pending = false;
                self.interrupt().await?;
                interrupted = true;
            }
            let line = if interrupted {
                match tokio::time::timeout(INTERRUPT_TIMEOUT, self.read_line()).await {
                    Ok(line) => line?,
                    Err(_) => {
                        return Err(ClaudeCodeError::Protocol(format!(
                            "the session did not end within {}s of the interrupt",
                            INTERRUPT_TIMEOUT.as_secs()
                        )));
                    }
                }
            } else {
                let reader = &mut self.stdout;
                tokio::select! {
                    read = reader.next_line() => line_of(read)?,
                    _ = cancel.cancelled() => {
                        interrupt_pending = true;
                        continue;
                    }
                }
            };
            // recorded before it is parsed: a line the parser refuses is in the log as
            // it arrived
            self.record(Channel::Stdout, &line)?;
            match parse(&line)? {
                CcLine::System {
                    subtype,
                    session_id,
                    model,
                    mcp_servers,
                    message,
                    tool_name,
                } => match subtype.as_str() {
                    "init" => {
                        if let Some(id) = session_id {
                            self.session_id = Some(id);
                        }
                        if let Some(m) = model {
                            self.model = Some(m);
                        }
                        if !self.checked {
                            check_servers(&mcp_servers)?;
                            self.checked = true;
                        }
                    }
                    "permission_denied" => {
                        let tool = tool_name.unwrap_or_else(|| "a tool".to_string());
                        let why = message.unwrap_or_else(|| "no reason given".to_string());
                        notice(
                            events,
                            format!("Claude Code denied {} itself: {why}", satz_tool(&tool)),
                        )
                        .await?;
                    }
                    "api_retry" => {
                        if let Some(message) = message {
                            notice(events, format!("Claude Code is retrying: {message}")).await?;
                        }
                    }
                    _ => {}
                },
                CcLine::StreamEvent { event } => {
                    for stream in fold.feed(&event)? {
                        if let Some(event) = fold.translate(stream) {
                            send(events, event).await?;
                        }
                    }
                }
                CcLine::User { message } => {
                    for result in message.tool_results() {
                        let name = fold.tool_name(&result.tool_use_id);
                        send(
                            events,
                            AgentEvent::ToolResult {
                                id: result.tool_use_id,
                                name,
                                outcome: ToolOutcome {
                                    structured: result.structured,
                                    text: result.text,
                                    is_error: result.is_error,
                                },
                                millis: 0,
                            },
                        )
                        .await?;
                    }
                }
                CcLine::RateLimitEvent { rate_limit_info } => {
                    notice(events, plan_line(&rate_limit_info)).await?;
                }
                CcLine::ControlRequest {
                    request_id,
                    request:
                        ControlRequest::CanUseTool {
                            tool_name,
                            input,
                            tool_use_id,
                        },
                } => {
                    let approval = self
                        .ask(&request_id, &tool_name, tool_use_id, input, events, cancel)
                        .await?;
                    if approval.is_none() {
                        interrupt_pending = true;
                    }
                }
                CcLine::Ended {
                    subtype,
                    is_error,
                    usage,
                    result,
                    session_id,
                } => {
                    if let Some(id) = session_id {
                        self.session_id = Some(id);
                    }
                    if interrupted {
                        return Err(ClaudeCodeError::Cancelled);
                    }
                    return self.ended(subtype, is_error, usage, result, events).await;
                }
                CcLine::Assistant { .. }
                | CcLine::ControlResponse { .. }
                | CcLine::ControlCancelRequest { .. }
                | CcLine::ControlRequest {
                    request: ControlRequest::Other,
                    ..
                }
                | CcLine::Other => {}
            }
        }
    }

    /// The turn's end, as the CLI reported it.
    async fn ended(
        &mut self,
        subtype: String,
        is_error: bool,
        usage: Usage,
        result: Option<String>,
        events: &mpsc::Sender<AgentEvent>,
    ) -> Result<(), ClaudeCodeError> {
        let stop_reason = match subtype.as_str() {
            "success" if !is_error => StopReason::EndTurn,
            // the turn ran into Claude Code's own turn cap. The answer up to it is
            // real and stays in the view; the stop reason says why it ends there.
            "error_max_turns" => StopReason::MaxTokens,
            other => {
                return Err(ClaudeCodeError::Turn(
                    result.unwrap_or_else(|| format!("the turn ended as `{other}`")),
                ));
            }
        };
        send(events, AgentEvent::TurnDone { stop_reason, usage }).await
    }

    /// Answer one `can_use_tool` request. `Ok(None)` means the operator cancelled
    /// while the card was up: the call is denied and the turn is interrupted.
    async fn ask(
        &mut self,
        request_id: &str,
        tool_name: &str,
        tool_use_id: Option<String>,
        input: serde_json::Value,
        events: &mpsc::Sender<AgentEvent>,
        cancel: &CancellationToken,
    ) -> Result<Option<Approval>, ClaudeCodeError> {
        if self.allowed.contains(tool_name) {
            self.allow(request_id, &input).await?;
            return Ok(Some(Approval::ForSession));
        }
        let (ack, answer) = oneshot::channel();
        send(
            events,
            AgentEvent::ToolCallPending {
                id: tool_use_id.unwrap_or_else(|| request_id.to_string()),
                name: satz_tool(tool_name).to_string(),
                input: input.clone(),
                approval: ack,
            },
        )
        .await?;
        let approval = tokio::select! {
            a = answer => a.map_err(|_| ClaudeCodeError::Protocol(
                "the approval card was dropped without an answer".to_string(),
            ))?,
            _ = cancel.cancelled() => {
                self.deny(request_id, "cancelled by the operator").await?;
                return Ok(None);
            }
        };
        match approval {
            Approval::Once => self.allow(request_id, &input).await?,
            Approval::ForSession => {
                self.allowed.insert(tool_name.to_string());
                self.allow(request_id, &input).await?;
            }
            Approval::Deny => self.deny(request_id, "denied by the operator").await?,
        }
        Ok(Some(approval))
    }

    async fn allow(
        &mut self,
        request_id: &str,
        input: &serde_json::Value,
    ) -> Result<(), ClaudeCodeError> {
        self.write(&serde_json::json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": request_id,
                "response": {"behavior": "allow", "updatedInput": input},
            },
        }))
        .await
    }

    async fn deny(&mut self, request_id: &str, why: &str) -> Result<(), ClaudeCodeError> {
        self.write(&serde_json::json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": request_id,
                "response": {"behavior": "deny", "message": why},
            },
        }))
        .await
    }

    async fn interrupt(&mut self) -> Result<(), ClaudeCodeError> {
        let id = self.next_request_id();
        self.write(&serde_json::json!({
            "type": "control_request",
            "request_id": id,
            "request": {"subtype": "interrupt"},
        }))
        .await
    }

    fn next_request_id(&mut self) -> String {
        self.requests += 1;
        format!("studio-{}", self.requests)
    }

    async fn write(&mut self, value: &serde_json::Value) -> Result<(), ClaudeCodeError> {
        let mut line = value.to_string();
        self.record(Channel::Stdin, &line)?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| self.broke("writing to the Claude Code session", e))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| self.broke("flushing the Claude Code session", e))
    }

    async fn read_line(&mut self) -> Result<String, ClaudeCodeError> {
        line_of(self.stdout.next_line().await)
    }

    /// One line into the stream log, when the log is on.
    fn record(&self, channel: Channel, line: &str) -> Result<(), ClaudeCodeError> {
        match &self.log {
            Some(log) => log.record(channel, line),
            None => Ok(()),
        }
    }

    fn broke(&self, context: &str, source: std::io::Error) -> ClaudeCodeError {
        ClaudeCodeError::Io {
            context: format!("{context}{}", self.said()),
            source,
        }
    }

    /// What the child last wrote to stderr, for an error message.
    fn said(&self) -> String {
        let lines = self
            .stderr
            .lock()
            .expect("the stderr lock is never poisoned");
        if lines.is_empty() {
            return String::new();
        }
        format!("\n{}", lines.join("\n"))
    }

    /// End the process: stdin closed, then killed if it does not leave.
    pub async fn close(mut self) {
        drop(self.stdin);
        let _ = tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await;
        let _ = self.child.start_kill();
    }
}

/// The satz server must be there: without it the session has no tools at all, and a
/// model that cannot call satz writes `.satz` files by hand — which is the one thing
/// the preamble forbids.
fn check_servers(servers: &[super::events::McpServer]) -> Result<(), ClaudeCodeError> {
    match servers.iter().find(|s| s.name == SERVER) {
        Some(server) if matches!(server.status.as_str(), "connected" | "pending") => Ok(()),
        Some(server) => Err(ClaudeCodeError::Protocol(format!(
            "the satz MCP server is `{}`, not connected",
            server.status
        ))),
        None => Err(ClaudeCodeError::Protocol(
            "Claude Code started without the satz MCP server".to_string(),
        )),
    }
}

/// The first record of a session's log: which app, which CLI, which estate, and the
/// command line the session runs.
fn log_header(cli: &ClaudeCodeCli, estate: &EstateSession, args: &[String]) -> serde_json::Value {
    serde_json::json!({
        "log": "satz-studio Claude Code stream log",
        "satz_studio": env!("CARGO_PKG_VERSION"),
        "claude_code": cli.version,
        "binary": cli.path.display().to_string(),
        "estate": estate.main.display().to_string(),
        "dir": estate.dir.dir.display().to_string(),
        "args": args,
    })
}

/// `error`, written into the log as the app's own note when the log is on — `what`
/// says what ended — so the log carries what the app made of the lines above it. A
/// note that cannot be written does not replace the error it is about: that error is
/// returned either way, and the log keeps the write failure for its next record.
fn noted(log: Option<&StreamLog>, what: &str, error: ClaudeCodeError) -> ClaudeCodeError {
    if let Some(log) = log {
        let _ = log.record(Channel::Studio, &format!("{what}: {error}"));
    }
    error
}

/// The plan line the Chat footer shows.
fn plan_line(limit: &RateLimit) -> String {
    limit.notice(now_seconds())
}

/// A satz tool under the name the estate's own session knows it by, so the approval
/// card and the tool cards read the same on both backends.
pub fn satz_tool(name: &str) -> &str {
    name.strip_prefix(TOOL_PREFIX).unwrap_or(name)
}

fn line_of(read: std::io::Result<Option<String>>) -> Result<String, ClaudeCodeError> {
    match read {
        Ok(Some(line)) => Ok(line),
        Ok(None) => Err(ClaudeCodeError::Protocol(
            "the Claude Code session closed its output".to_string(),
        )),
        Err(e) => Err(ClaudeCodeError::Io {
            context: "reading the Claude Code session".to_string(),
            source: e,
        }),
    }
}

fn parse(line: &str) -> Result<CcLine, ClaudeCodeError> {
    serde_json::from_str(line).map_err(|e| {
        ClaudeCodeError::Protocol(format!(
            "a line of the Claude Code stream is not one this app reads: {e}"
        ))
    })
}

async fn send(events: &mpsc::Sender<AgentEvent>, event: AgentEvent) -> Result<(), ClaudeCodeError> {
    events
        .send(event)
        .await
        .map_err(|_| ClaudeCodeError::Cancelled)
}

async fn notice(events: &mpsc::Sender<AgentEvent>, text: String) -> Result<(), ClaudeCodeError> {
    send(events, AgentEvent::Notice(text)).await
}

/// The stream of one turn: one [`Assembler`] per assistant message, the tool names the
/// cards and the results are shown under, and the block index a tool call sits at.
#[derive(Default)]
struct Fold {
    assembler: Option<Assembler>,
    /// block index → tool call id, within the message being assembled
    indexes: std::collections::BTreeMap<usize, String>,
    /// tool call id → the name the app shows
    names: std::collections::BTreeMap<String, String>,
}

impl Fold {
    /// Feed one of Claude Code's `stream_event` payloads. A `message_start` opens a new
    /// assembler: one turn is many assistant messages, and each is its own stream.
    fn feed(&mut self, event: &serde_json::Value) -> Result<Vec<StreamEvent>, ClaudeCodeError> {
        let kind = event
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        if kind == "message_start" {
            self.assembler = Some(Assembler::new());
            self.indexes.clear();
        }
        let Some(assembler) = &mut self.assembler else {
            return Err(ClaudeCodeError::Protocol(format!(
                "stream event `{kind}` arrived before message_start"
            )));
        };
        assembler
            .feed(&SseEvent {
                event: kind,
                data: event.to_string(),
            })
            .map_err(|e| ClaudeCodeError::Protocol(e.to_string()))
    }

    /// One stream event as the Chat view's event, or nothing when the view has no use
    /// for it: the assembled blocks and the message's end are Claude Code's own
    /// bookkeeping, and the turn ends on the CLI's `result` line.
    fn translate(&mut self, event: StreamEvent) -> Option<AgentEvent> {
        match event {
            StreamEvent::Started { .. } => Some(AgentEvent::Started),
            StreamEvent::TextDelta(text) => Some(AgentEvent::TextDelta(text)),
            StreamEvent::ThinkingDelta(text) => Some(AgentEvent::ThinkingDelta(text)),
            StreamEvent::ToolUseStart { index, id, name } => {
                let name = satz_tool(&name).to_string();
                self.indexes.insert(index, id.clone());
                self.names.insert(id.clone(), name.clone());
                Some(AgentEvent::ToolUseStarted { id, name })
            }
            StreamEvent::ToolInputDelta {
                index,
                partial_json,
            } => self
                .indexes
                .get(&index)
                .map(|id| AgentEvent::ToolInputDelta {
                    id: id.clone(),
                    partial_json,
                }),
            StreamEvent::BlockStop { .. } | StreamEvent::Done { .. } => None,
            StreamEvent::Error(text) => Some(AgentEvent::Notice(format!(
                "Claude Code reported a stream error: {text}"
            ))),
        }
    }

    /// The name a tool result is shown under: the call's, when the stream carried it.
    fn tool_name(&self, id: &str) -> String {
        self.names
            .get(id)
            .cloned()
            .unwrap_or_else(|| "a tool".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::satz::ToolAnnotations;

    fn tool(name: &str, read_only: bool, destructive: bool) -> ToolInfo {
        ToolInfo {
            name: name.to_string(),
            description: String::new(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            annotations: ToolAnnotations {
                read_only: Some(read_only),
                destructive: Some(destructive),
                idempotent: None,
                open_world: None,
            },
        }
    }

    #[test]
    fn the_allowlist_is_the_read_only_tools_and_the_safe_writes_when_asked() {
        let tools = [
            tool("satz_questions", true, false),
            tool("satz_interview", false, false),
            tool("satz_get_presets", false, true),
        ];
        assert_eq!(
            allowed_tools(&tools, false),
            vec!["mcp__satz__satz_questions"]
        );
        assert_eq!(
            allowed_tools(&tools, true),
            vec!["mcp__satz__satz_interview", "mcp__satz__satz_questions"]
        );
    }

    #[test]
    fn a_tool_is_shown_under_the_name_satz_gives_it() {
        assert_eq!(satz_tool("mcp__satz__satz_interview"), "satz_interview");
        assert_eq!(satz_tool("Bash"), "Bash");
    }
}
