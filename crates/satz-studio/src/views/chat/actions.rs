//! The chat coroutine: it owns the [`Engine`], runs a turn on a tokio task while every
//! [`AgentEvent`] folds into the store, parks the approval sender until the operator
//! answers, cancels, and keeps the transcript. One turn at a time; an action that
//! needs the engine while a turn runs is refused with a visible error.
//!
//! Two engines serve the same view (ADR 0010). [`Engine::Api`] is the app's own loop
//! over the Messages API; [`Engine::ClaudeCode`] is the installed Claude Code CLI on
//! the user's claude.ai subscription. Send, Cancel and Approve work for both; what
//! differs is the transcript — the API engine holds the conversation as `Vec<Message>`
//! and writes it to a file, Claude Code holds its own and the app keeps none.

use std::path::PathBuf;
use std::sync::Arc;

use dioxus::prelude::*;
use futures_util::StreamExt;
use satz_studio_core::llm::claude_code::{
    ClaudeCodeCli, Session as CcSession, SessionOptions, StreamLogConfig,
};
use satz_studio_core::llm::{
    Agent, AgentEvent, Approval, Capabilities, ChatProvider, ClaudeClient, ClaudeError, Credential,
    Effort, EstateContext, Message, Ollama, OpenAiCompat,
};
use satz_studio_core::satz::EstateSession;
use satz_studio_core::settings::{ProviderChoice, Settings};
use satz_studio_core::transcript::{Transcript, TranscriptStore};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::state::{
    AgentStatus, ChatStore, ChatStoreStoreExt, ContextInput, EngineKind, apply_delta,
    estate_context, is_delta, replay,
};
use crate::state::{AppStore, AppStoreStoreExt, EstateStoreStoreExt, save_settings};

pub enum ChatAction {
    Send(String),
    Cancel,
    /// the operator's answer to the approval card
    Approve(Approval),
    NewTranscript,
    Resume(PathBuf),
    /// a model switch starts a new transcript (ADR 0008)
    SetModel(String),
    SetEffort(Effort),
    /// build the engine again — after a Claude Code sign-in, or a failed start
    Restart,
    /// the empty state's "Use Claude Code": select that engine in the settings, save
    /// them, and build the engine again
    UseClaudeCode,
    /// the footer's switch: show or hide the debug panel, kept in the settings
    SetDebugLog(bool),
}

/// How many events the engine may run ahead of the view.
const EVENT_BUFFER: usize = 64;

/// What serves the chat. Both raise the same [`AgentEvent`]s.
enum Engine {
    Api(Box<Agent>),
    ClaudeCode(Box<CcSession>),
}

impl Engine {
    fn kind(&self) -> EngineKind {
        match self {
            Engine::Api(_) => EngineKind::Api,
            Engine::ClaudeCode(_) => EngineKind::ClaudeCode,
        }
    }

    fn model(&self) -> String {
        match self {
            Engine::Api(agent) => agent.model.clone(),
            // Claude Code names the model at its first turn; until then it is the
            // one Settings asked for, or Claude Code's own default
            Engine::ClaudeCode(session) => session
                .model()
                .unwrap_or("the Claude Code default")
                .to_string(),
        }
    }

    fn capabilities(&self) -> Capabilities {
        match self {
            Engine::Api(agent) => agent.provider.capabilities(),
            // the tools are the estate's either way; effort is Claude Code's own
            Engine::ClaudeCode(_) => Capabilities {
                tools: true,
                thinking: true,
                effort: false,
                cache_control: true,
            },
        }
    }

    /// Where the next turn's messages start in the transcript the engine keeps, when
    /// it keeps one.
    fn transcript_start(&self) -> Option<usize> {
        match self {
            Engine::Api(agent) => Some(agent.messages.len()),
            Engine::ClaudeCode(_) => None,
        }
    }

    async fn run_turn(
        &mut self,
        text: String,
        events: mpsc::Sender<AgentEvent>,
        cancel: CancellationToken,
    ) -> Result<(), ClaudeError> {
        match self {
            Engine::Api(agent) => agent.run_turn(text, events, cancel).await,
            Engine::ClaudeCode(session) => session
                .run_turn(text, events, cancel)
                .await
                .map_err(ClaudeError::from),
        }
    }

    /// End the engine: a Claude Code process is closed, the API agent has nothing to
    /// close.
    async fn close(self) {
        match self {
            Engine::Api(_) => {}
            Engine::ClaudeCode(session) => session.close().await,
        }
    }
}

/// A turn in flight: the engine lives on its task until the task returns it.
struct Running {
    events: mpsc::Receiver<AgentEvent>,
    join: JoinHandle<(Engine, Result<(), ClaudeError>)>,
    cancel: CancellationToken,
    /// where the turn began in the API engine's transcript; `None` for Claude Code,
    /// which keeps its own
    start: Option<usize>,
}

struct Chat {
    app_store: Store<AppStore>,
    chat: Store<ChatStore>,
    session: Arc<EstateSession>,
    /// `None` when the app's data directory is unavailable; Send then refuses while
    /// persistence is on
    store: Option<TranscriptStore>,
    /// `None` while a turn runs (the task holds it) and when it could not be built
    engine: Option<Engine>,
    running: Option<Running>,
    /// the approval card's answer channel, parked until the operator clicks
    approval: Option<oneshot::Sender<Approval>>,
    /// the transcript the conversation is appended to; `None` until the first turn,
    /// while persistence is off, and always on the Claude Code engine
    transcript: Option<Transcript>,
}

pub async fn chat_coroutine(
    mut rx: UnboundedReceiver<ChatAction>,
    app: Store<AppStore>,
    chat: Store<ChatStore>,
    session: Arc<EstateSession>,
) {
    // the estate's `satz mcp` stderr, for the debug log; `None` once it closed
    let mut stderr = Some(session.mcp_stderr());
    let mut state = Chat::start(app, chat, session).await;
    loop {
        tokio::select! {
            action = rx.next() => match action {
                Some(action) => state.handle(action).await,
                None => break,
            },
            event = state.next_event() => match event {
                Some(event) => state.reduce(event),
                None => state.turn_ended().await,
            },
            line = next_stderr(&mut stderr) => match line {
                Err(RecvError::Closed) => stderr = None,
                line => state.stderr_line(line),
            },
        }
    }
}

impl Chat {
    async fn start(
        app: Store<AppStore>,
        chat: Store<ChatStore>,
        session: Arc<EstateSession>,
    ) -> Self {
        let mut me = Chat {
            app_store: app,
            chat,
            session,
            store: None,
            engine: None,
            running: None,
            approval: None,
            transcript: None,
        };
        match TranscriptStore::open_default() {
            Ok(store) => me.store = Some(store),
            Err(e) => me.fail(format!("transcripts unavailable: {e}")),
        }
        me.refresh_transcripts();
        me.build().await;
        me
    }

    /// Build the engine from Settings and show what it is, or why it is not there.
    async fn build(&mut self) {
        let settings = self.app_store.settings().cloned();
        let satz = self
            .app_store
            .satz()
            .read()
            .binary()
            .map(|b| b.path.clone());
        self.chat.agent().set(AgentStatus::Starting);
        match build_engine(&settings, &self.session, satz).await {
            Ok(engine) => {
                self.chat.capabilities().set(engine.capabilities());
                self.chat.model().set(engine.model());
                self.chat.engine().set(engine.kind());
                self.chat.effort().set(settings.effort);
                self.chat.agent().set(AgentStatus::Ready);
                self.engine = Some(engine);
            }
            Err(status) => self.chat.agent().set(status),
        }
    }

    /// The next event of the running turn; forever pending while none runs, so the
    /// select above only ever wakes for actions.
    async fn next_event(&mut self) -> Option<AgentEvent> {
        match &mut self.running {
            Some(running) => running.events.recv().await,
            None => std::future::pending().await,
        }
    }

    /// A stderr line goes to the debug log's open call, when one is open. A log that
    /// fell behind says how many lines it lost.
    fn stderr_line(&mut self, line: Result<String, RecvError>) {
        let line = match line {
            Ok(line) => line,
            Err(RecvError::Lagged(n)) => format!("[{n} stderr lines lost: the log fell behind]"),
            // the caller stops listening; the estate session reports its own end
            Err(RecvError::Closed) => return,
        };
        if self.chat.read().has_open_call() {
            self.chat.write().debug_stderr(line);
        }
    }

    async fn handle(&mut self, action: ChatAction) {
        match action {
            ChatAction::Send(text) => self.send(text),
            ChatAction::Cancel => self.cancel(),
            ChatAction::Approve(approval) => self.approve(approval),
            ChatAction::NewTranscript => self.new_transcript().await,
            ChatAction::Resume(path) => self.resume(path),
            ChatAction::SetModel(model) => self.set_model(model).await,
            ChatAction::SetEffort(effort) => self.set_effort(effort),
            ChatAction::Restart => self.restart().await,
            ChatAction::UseClaudeCode => self.use_claude_code().await,
            ChatAction::SetDebugLog(on) => self.set_debug_log(on),
        }
    }

    /// A delta goes to the `streaming` field alone; everything else through the
    /// reducer, which hands back the approval sender to park.
    fn reduce(&mut self, event: AgentEvent) {
        if is_delta(&event) {
            if let Err(e) = apply_delta(&mut self.chat.streaming().write(), event) {
                self.chat.error().set(Some(e));
            }
            return;
        }
        if let Some(sender) = self.chat.write().apply(event) {
            self.approval = Some(sender);
        }
    }

    fn fail(&mut self, message: impl Into<String>) {
        self.chat.error().set(Some(message.into()));
    }

    fn refresh_transcripts(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        match store.list(&self.session.main) {
            Ok(list) => self.chat.transcripts().set(list),
            Err(e) => self.fail(format!("transcripts not listed: {e}")),
        }
    }

    /// The volatile system context, from what the estate store holds right now.
    fn context(&self) -> EstateContext {
        let estate = self.app_store.estate();
        let model = estate.model().cloned();
        let questions = estate.questions().cloned();
        let diagnostics = estate.diagnostics().cloned();
        estate_context(&ContextInput {
            main: &self.session.main,
            dir: &self.session.dir.dir,
            runs_as: self.session.runs_as(),
            deployment_mode: self.session.deployment_mode(),
            model: model.as_deref(),
            questions: questions.as_ref(),
            diagnostics: &diagnostics,
        })
    }

    fn send(&mut self, text: String) {
        if self.running.is_some() {
            return self.fail("a turn is already running");
        }
        let Some(mut engine) = self.engine.take() else {
            return self.fail("no engine: see the card above");
        };
        let text = text.trim().to_string();
        if text.is_empty() {
            self.engine = Some(engine);
            return;
        }
        if let Engine::Api(agent) = &mut engine {
            agent.set_context(self.context());
        }

        let start = engine.transcript_start();
        if start.is_some() && !self.open_transcript(&engine) {
            self.engine = Some(engine);
            return;
        }

        let (tx, events) = mpsc::channel(EVENT_BUFFER);
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let user_text = text.clone();
        let join = tokio::spawn(async move {
            let result = engine.run_turn(user_text, tx, token).await;
            (engine, result)
        });
        self.chat.write().begin_turn(text);
        self.running = Some(Running {
            events,
            join,
            cancel,
            start,
        });
    }

    /// The transcript this turn is appended to, opened if it is wanted and not open.
    /// `false` means Send must not proceed: the operator asked for a transcript and
    /// there is none. Persistence is the setting at the time of each turn.
    fn open_transcript(&mut self, engine: &Engine) -> bool {
        let persist = self.app_store.settings().read().persist_transcripts;
        if !persist && self.transcript.is_some() {
            self.transcript = None;
            self.chat.transcript().set(None);
        }
        if !persist || self.transcript.is_some() {
            return true;
        }
        let created = self
            .store
            .as_ref()
            .ok_or_else(|| "the app's data directory is unavailable".to_string())
            .and_then(|store| {
                store
                    .create(&self.session.main, &engine.model())
                    .map_err(|e| e.to_string())
            });
        match created {
            Ok(transcript) => {
                self.chat.transcript().set(Some(transcript.path.clone()));
                self.transcript = Some(transcript);
                self.refresh_transcripts();
                true
            }
            Err(e) => {
                self.fail(format!(
                    "transcript not created: {e} (switch persistence off in Settings to chat without one)"
                ));
                false
            }
        }
    }

    /// The event channel closed: the task is done and returns the engine. A turn that
    /// completed is appended to the transcript; a refused, failed or cancelled one
    /// left the messages as they were and writes nothing.
    async fn turn_ended(&mut self) {
        let Some(running) = self.running.take() else {
            return self.fail("the turn ended, but none was running");
        };
        self.approval = None;
        match running.join.await {
            Ok((engine, result)) => {
                if let (Ok(()), Some(start), Engine::Api(agent)) = (&result, running.start, &engine)
                {
                    self.persist(&agent.messages[start..]);
                }
                // Claude Code names its model at its first turn, not at spawn
                self.chat.model().set(engine.model());
                self.engine = Some(engine);
            }
            Err(e) => self.chat.agent().set(AgentStatus::Failed(format!(
                "the turn task ended abnormally: {e}; close and reopen the estate"
            ))),
        }
        self.chat.write().turn_closed();
    }

    fn persist(&mut self, messages: &[Message]) {
        let (Some(store), Some(transcript)) = (&self.store, &mut self.transcript) else {
            return;
        };
        for message in messages {
            if let Err(e) = store.append(transcript, message.clone()) {
                self.transcript = None;
                self.chat.transcript().set(None);
                return self.fail(format!(
                    "transcript not written: {e}; the rest of this conversation is not kept"
                ));
            }
        }
    }

    fn cancel(&mut self) {
        match &self.running {
            Some(running) => running.cancel.cancel(),
            None => self.fail("no turn is running"),
        }
    }

    fn approve(&mut self, approval: Approval) {
        match self.approval.take() {
            // a receiver that is gone means the turn already ended: nothing to answer
            Some(sender) => {
                let _ = sender.send(approval);
                self.chat.pending().set(None);
            }
            None => self.fail("no approval is pending"),
        }
    }

    /// Build the engine again: what the Claude Code empty state's "Check again" does,
    /// and what a model change on that engine needs, since its conversation lives in
    /// the process.
    async fn restart(&mut self) {
        if self.running.is_some() {
            return self.fail("wait for the turn to end");
        }
        if let Some(engine) = self.engine.take() {
            engine.close().await;
        }
        self.transcript = None;
        self.build().await;
        // a build that failed leaves the status card in charge; the footer keeps the
        // model it had rather than showing an empty one
        let model = match self.engine.as_ref() {
            Some(engine) => engine.model(),
            None => self.chat.model().cloned(),
        };
        self.chat.write().reset_conversation(model);
    }

    /// Select the Claude Code engine and start the chat on it. The settings go through
    /// the app's own saver, so the file and the store hold what Settings would hold;
    /// a save that failed leaves the engine where it was and says so.
    async fn use_claude_code(&mut self) {
        if self.running.is_some() {
            return self.fail("wait for the turn to end");
        }
        let mut settings = self.app_store.settings().cloned();
        settings.provider = ProviderChoice::ClaudeCode { model: None };
        match save_settings(self.app_store, settings).await {
            Ok(()) => self.restart().await,
            Err(e) => self.fail(e),
        }
    }

    /// The debug panel on or off. Only this field changes, so the settings file is
    /// written directly and nothing is located again; a write that failed leaves the
    /// panel as it was and says so.
    fn set_debug_log(&mut self, on: bool) {
        let mut settings = self.app_store.settings().cloned();
        settings.chat_debug_log = on;
        match settings.save() {
            Ok(()) => self.app_store.settings().set(settings),
            Err(e) => self.fail(format!("settings not saved: {e}")),
        }
    }

    async fn new_transcript(&mut self) {
        if self.running.is_some() {
            return self.fail("wait for the turn to end");
        }
        match &mut self.engine {
            // Claude Code holds the conversation in its process: a new one is a new
            // process
            Some(Engine::ClaudeCode(_)) => self.restart().await,
            Some(Engine::Api(agent)) => {
                agent.messages.clear();
                self.transcript = None;
                let model = agent.model.clone();
                self.chat.write().reset_conversation(model);
            }
            None => self.fail("no engine"),
        }
    }

    /// Replay a transcript into the agent and the view, on the model that wrote it.
    fn resume(&mut self, path: PathBuf) {
        if self.running.is_some() {
            return self.fail("wait for the turn to end");
        }
        let Some(store) = &self.store else {
            return self.fail("the app's data directory is unavailable");
        };
        let transcript = match store.load(&path) {
            Ok(t) => t,
            Err(e) => return self.fail(e.to_string()),
        };
        let replayed = match replay(&transcript.messages) {
            Ok(r) => r,
            Err(e) => return self.fail(format!("{}: {e}", path.display())),
        };
        let Some(Engine::Api(agent)) = &mut self.engine else {
            return self.fail(
                "only the Messages API engine resumes a transcript; Claude Code keeps its own conversation",
            );
        };
        let model = transcript.header.model.clone();
        if agent.model != model {
            let choice = self.app_store.settings().read().provider.clone();
            apply_model(agent, &choice, &model);
        }
        agent.messages = transcript.messages.clone();
        self.chat.write().load_conversation(replayed, path, model);
        // with persistence off the conversation continues in memory alone, and the
        // footer says so
        let persist = self.app_store.settings().read().persist_transcripts;
        if persist {
            self.transcript = Some(transcript);
        } else {
            self.transcript = None;
            self.chat.transcript().set(None);
        }
    }

    async fn set_model(&mut self, model: String) {
        if self.running.is_some() {
            return self.fail("wait for the turn to end before changing the model");
        }
        let model = model.trim().to_string();
        if model.is_empty() {
            return self.fail("the model name is empty");
        }
        match &mut self.engine {
            // the model is a command-line argument of the process: a change is a new
            // session, and Settings is where it is kept
            Some(Engine::ClaudeCode(_)) => self.fail(
                "Claude Code takes its model from Settings; change it there and use New to start a session on it",
            ),
            Some(Engine::Api(agent)) => {
                if agent.model == model {
                    return;
                }
                let choice = self.app_store.settings().read().provider.clone();
                apply_model(agent, &choice, &model);
                agent.messages.clear();
                self.transcript = None;
                self.chat.write().reset_conversation(model);
            }
            None => self.fail("no engine"),
        }
    }

    fn set_effort(&mut self, effort: Effort) {
        if self.running.is_some() {
            return self.fail("wait for the turn to end before changing the effort");
        }
        match &mut self.engine {
            Some(Engine::ClaudeCode(_)) => self.fail("Claude Code sets its own effort"),
            Some(Engine::Api(agent)) => {
                agent.effort = effort;
                self.chat.effort().set(effort);
            }
            None => self.fail("no engine"),
        }
    }
}

/// The next stderr line of the estate's `satz mcp`; forever pending once it closed.
async fn next_stderr(rx: &mut Option<broadcast::Receiver<String>>) -> Result<String, RecvError> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

/// The engine from Settings. Claude needs a credential; the other providers need a
/// model name; Claude Code needs an installed CLI that is signed in, and satz, whose
/// MCP server it is given.
async fn build_engine(
    settings: &Settings,
    session: &Arc<EstateSession>,
    satz_binary: Option<PathBuf>,
) -> Result<Engine, AgentStatus> {
    if let ProviderChoice::ClaudeCode { model } = &settings.provider {
        return claude_code(settings, session, satz_binary, model.clone()).await;
    }
    let (provider, model): (Arc<dyn ChatProvider>, String) = match &settings.provider {
        ProviderChoice::Claude => match Credential::resolve().await {
            Ok((credential, _)) => (
                Arc::new(ClaudeClient::new(credential)),
                settings.model.clone(),
            ),
            Err(ClaudeError::NoCredential { tried }) => {
                return Err(AgentStatus::NoCredential { tried });
            }
            Err(e) => return Err(AgentStatus::Failed(e.to_string())),
        },
        ProviderChoice::OpenAiCompat { base_url, model } => (
            Arc::new(OpenAiCompat::new(base_url, None, model)),
            model.clone(),
        ),
        ProviderChoice::Ollama { base_url, model } => {
            (Arc::new(Ollama::new(base_url, model)), model.clone())
        }
        ProviderChoice::ClaudeCode { .. } => unreachable!("handled above"),
    };
    if model.trim().is_empty() {
        return Err(AgentStatus::Failed(
            "the model is not set: name it in Settings".to_string(),
        ));
    }
    let mut agent = Agent::new(provider, Arc::clone(session), model, settings.effort);
    agent.fallbacks = settings.fallbacks;
    agent.auto_approve_writes = settings.auto_approve_writes;
    Ok(Engine::Api(Box::new(agent)))
}

/// The Claude Code engine: the CLI located, its login checked, and a process spawned
/// on this estate.
async fn claude_code(
    settings: &Settings,
    session: &Arc<EstateSession>,
    satz_binary: Option<PathBuf>,
    model: Option<String>,
) -> Result<Engine, AgentStatus> {
    let cli = ClaudeCodeCli::locate(settings.claude_code_binary.as_deref())
        .await
        .map_err(|e| AgentStatus::Failed(e.to_string()))?;
    let status = cli
        .auth_status()
        .await
        .map_err(|e| AgentStatus::Failed(e.to_string()))?;
    if !status.logged_in {
        return Err(AgentStatus::NotSignedIn {
            login: cli.login_command(),
        });
    }
    let Some(satz_binary) = satz_binary else {
        return Err(AgentStatus::Failed(
            "satz is not available, and Claude Code is given the estate's satz MCP server — see the banner".to_string(),
        ));
    };
    let log = if settings.claude_code_log {
        let dir = StreamLogConfig::default_dir().map_err(|e| {
            AgentStatus::Failed(format!(
                "the Claude Code log is on and has nowhere to go: {e}; switch it off in Settings to chat without it"
            ))
        })?;
        Some(StreamLogConfig::new(dir))
    } else {
        None
    };
    let options = SessionOptions {
        model,
        max_turns: None,
        resume: None,
        auto_approve_writes: settings.auto_approve_writes,
        satz_binary,
        allow: settings.mcp_allow,
        log,
    };
    let spawned = CcSession::spawn(&cli, Arc::clone(session), options)
        .await
        .map_err(|e| AgentStatus::Failed(e.to_string()))?;
    Ok(Engine::ClaudeCode(Box::new(spawned)))
}

/// Point the agent at another model. Claude reads the model from the request; the
/// other providers send their own, so their adapter is rebuilt with it.
fn apply_model(agent: &mut Agent, choice: &ProviderChoice, model: &str) {
    agent.model = model.to_string();
    match choice {
        ProviderChoice::Claude | ProviderChoice::ClaudeCode { .. } => {}
        ProviderChoice::OpenAiCompat { base_url, .. } => {
            agent.provider = Arc::new(OpenAiCompat::new(base_url, None, model));
        }
        ProviderChoice::Ollama { base_url, .. } => {
            agent.provider = Arc::new(Ollama::new(base_url, model));
        }
    }
}
