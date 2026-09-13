//! The chat coroutine: it owns the [`Agent`], runs a turn on a tokio task while every
//! [`AgentEvent`] folds into the store, parks the approval sender until the operator
//! answers, cancels, and keeps the transcript. One turn at a time; an action that
//! needs the agent while a turn runs is refused with a visible error.

use std::path::PathBuf;
use std::sync::Arc;

use dioxus::prelude::*;
use futures_util::StreamExt;
use satz_studio_core::llm::{
    Agent, AgentEvent, Approval, ChatProvider, ClaudeClient, ClaudeError, Credential, Effort,
    EstateContext, Message, Ollama, OpenAiCompat,
};
use satz_studio_core::satz::EstateSession;
use satz_studio_core::settings::{ProviderChoice, Settings};
use satz_studio_core::transcript::{Transcript, TranscriptStore};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::state::{
    AgentStatus, ChatStore, ChatStoreStoreExt, ContextInput, apply_delta, estate_context, is_delta,
    replay,
};
use crate::state::{AppStore, AppStoreStoreExt, EstateStoreStoreExt};

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
}

/// How many events the agent may run ahead of the view.
const EVENT_BUFFER: usize = 64;

/// A turn in flight: the agent lives on its task until the task returns it.
struct Running {
    events: mpsc::Receiver<AgentEvent>,
    join: JoinHandle<(Agent, Result<(), ClaudeError>)>,
    cancel: CancellationToken,
    /// `agent.messages.len()` when the turn began: what follows is the turn's
    start: usize,
}

struct Chat {
    app_store: Store<AppStore>,
    chat: Store<ChatStore>,
    session: Arc<EstateSession>,
    /// `None` when the app's data directory is unavailable; Send then refuses while
    /// persistence is on
    store: Option<TranscriptStore>,
    /// `None` while a turn runs (the task holds it) and when it could not be built
    agent: Option<Agent>,
    running: Option<Running>,
    /// the approval card's answer channel, parked until the operator clicks
    approval: Option<oneshot::Sender<Approval>>,
    /// the transcript the conversation is appended to; `None` until the first turn,
    /// and always while persistence is off
    transcript: Option<Transcript>,
}

pub async fn chat_coroutine(
    mut rx: UnboundedReceiver<ChatAction>,
    app: Store<AppStore>,
    chat: Store<ChatStore>,
    session: Arc<EstateSession>,
) {
    let mut state = Chat::start(app, chat, session).await;
    loop {
        tokio::select! {
            action = rx.next() => match action {
                Some(action) => state.handle(action),
                None => break,
            },
            event = state.next_event() => match event {
                Some(event) => state.reduce(event),
                None => state.turn_ended().await,
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
            agent: None,
            running: None,
            approval: None,
            transcript: None,
        };
        match TranscriptStore::open_default() {
            Ok(store) => me.store = Some(store),
            Err(e) => me.fail(format!("transcripts unavailable: {e}")),
        }
        me.refresh_transcripts();
        let settings = app.settings().cloned();
        match build_agent(&settings, &me.session).await {
            Ok(agent) => {
                chat.capabilities().set(agent.provider.capabilities());
                chat.model().set(agent.model.clone());
                chat.effort().set(agent.effort);
                chat.agent().set(AgentStatus::Ready);
                me.agent = Some(agent);
            }
            Err(status) => chat.agent().set(status),
        }
        me
    }

    /// The next event of the running turn; forever pending while none runs, so the
    /// select above only ever wakes for actions.
    async fn next_event(&mut self) -> Option<AgentEvent> {
        match &mut self.running {
            Some(running) => running.events.recv().await,
            None => std::future::pending().await,
        }
    }

    fn handle(&mut self, action: ChatAction) {
        match action {
            ChatAction::Send(text) => self.send(text),
            ChatAction::Cancel => self.cancel(),
            ChatAction::Approve(approval) => self.approve(approval),
            ChatAction::NewTranscript => self.new_transcript(),
            ChatAction::Resume(path) => self.resume(path),
            ChatAction::SetModel(model) => self.set_model(model),
            ChatAction::SetEffort(effort) => self.set_effort(effort),
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
        let Some(mut agent) = self.agent.take() else {
            return self.fail("no agent: see the card above");
        };
        let text = text.trim().to_string();
        if text.is_empty() {
            self.agent = Some(agent);
            return;
        }
        agent.set_context(self.context());

        // persistence is the setting at the time of each turn: off drops the file,
        // on without one opens one
        let persist = self.app_store.settings().read().persist_transcripts;
        if !persist && self.transcript.is_some() {
            self.transcript = None;
            self.chat.transcript().set(None);
        }
        if persist && self.transcript.is_none() {
            let created = self
                .store
                .as_ref()
                .ok_or_else(|| "the app's data directory is unavailable".to_string())
                .and_then(|store| {
                    store
                        .create(&self.session.main, &agent.model)
                        .map_err(|e| e.to_string())
                });
            match created {
                Ok(transcript) => {
                    self.chat.transcript().set(Some(transcript.path.clone()));
                    self.transcript = Some(transcript);
                    self.refresh_transcripts();
                }
                Err(e) => {
                    self.agent = Some(agent);
                    return self.fail(format!(
                        "transcript not created: {e} (switch persistence off in Settings to chat without one)"
                    ));
                }
            }
        }

        let start = agent.messages.len();
        let (tx, events) = mpsc::channel(EVENT_BUFFER);
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let user_text = text.clone();
        let join = tokio::spawn(async move {
            let result = agent.run_turn(user_text, tx, token).await;
            (agent, result)
        });
        self.chat.write().begin_turn(text);
        self.running = Some(Running {
            events,
            join,
            cancel,
            start,
        });
    }

    /// The event channel closed: the task is done and returns the agent. A turn that
    /// completed is appended to the transcript; a refused, failed or cancelled one
    /// left the agent's messages as they were and writes nothing.
    async fn turn_ended(&mut self) {
        let Some(running) = self.running.take() else {
            return self.fail("the turn ended, but none was running");
        };
        self.approval = None;
        match running.join.await {
            Ok((agent, result)) => {
                if result.is_ok() {
                    self.persist(&agent.messages[running.start..]);
                }
                self.agent = Some(agent);
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

    fn new_transcript(&mut self) {
        if self.running.is_some() {
            return self.fail("wait for the turn to end");
        }
        let Some(agent) = &mut self.agent else {
            return self.fail("no agent");
        };
        agent.messages.clear();
        self.transcript = None;
        let model = agent.model.clone();
        self.chat.write().reset_conversation(model);
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
        let turns = match replay(&transcript.messages) {
            Ok(t) => t,
            Err(e) => return self.fail(format!("{}: {e}", path.display())),
        };
        let Some(agent) = &mut self.agent else {
            return self.fail("no agent");
        };
        let model = transcript.header.model.clone();
        if agent.model != model {
            let choice = self.app_store.settings().read().provider.clone();
            apply_model(agent, &choice, &model);
        }
        agent.messages = transcript.messages.clone();
        self.chat.write().load_conversation(turns, path, model);
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

    fn set_model(&mut self, model: String) {
        if self.running.is_some() {
            return self.fail("wait for the turn to end before changing the model");
        }
        let model = model.trim().to_string();
        if model.is_empty() {
            return self.fail("the model name is empty");
        }
        let Some(agent) = &mut self.agent else {
            return self.fail("no agent");
        };
        if agent.model == model {
            return;
        }
        let choice = self.app_store.settings().read().provider.clone();
        apply_model(agent, &choice, &model);
        agent.messages.clear();
        self.transcript = None;
        self.chat.write().reset_conversation(model);
    }

    fn set_effort(&mut self, effort: Effort) {
        if self.running.is_some() {
            return self.fail("wait for the turn to end before changing the effort");
        }
        let Some(agent) = &mut self.agent else {
            return self.fail("no agent");
        };
        agent.effort = effort;
        self.chat.effort().set(effort);
    }
}

/// The provider from Settings and the agent over it. Claude needs a credential;
/// the other providers need a model name.
async fn build_agent(
    settings: &Settings,
    session: &Arc<EstateSession>,
) -> Result<Agent, AgentStatus> {
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
    };
    if model.trim().is_empty() {
        return Err(AgentStatus::Failed(
            "the model is not set: name it in Settings".to_string(),
        ));
    }
    let mut agent = Agent::new(provider, Arc::clone(session), model, settings.effort);
    agent.fallbacks = settings.fallbacks;
    agent.auto_approve_writes = settings.auto_approve_writes;
    Ok(agent)
}

/// Point the agent at another model. Claude reads the model from the request; the
/// other providers send their own, so their adapter is rebuilt with it.
fn apply_model(agent: &mut Agent, choice: &ProviderChoice, model: &str) {
    agent.model = model.to_string();
    match choice {
        ProviderChoice::Claude => {}
        ProviderChoice::OpenAiCompat { base_url, .. } => {
            agent.provider = Arc::new(OpenAiCompat::new(base_url, None, model));
        }
        ProviderChoice::Ollama { base_url, .. } => {
            agent.provider = Arc::new(Ollama::new(base_url, model));
        }
    }
}
