//! The stores and the two coroutines. [`AppStore`] is the whole application state,
//! provided at the root and read through the accessors `#[derive(Store)]` generates, so
//! a log line does not re-render the rail. Every side effect — locating satz, walking a
//! folder, opening a session, running a command — happens in [`app_coroutine`] or in
//! the per-estate coroutine ([`estate_coroutine`]); the stores are written from there,
//! and nothing blocks in an event handler.

mod ansi;
mod app_actions;
mod estate_actions;
mod toast;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use dioxus::prelude::*;
use satz_studio_core::diag::Diagnostic;
use satz_studio_core::llm::CredentialSource;
use satz_studio_core::model::EstateModel;
use satz_studio_core::satz::reports::QuestionsReport;
use satz_studio_core::satz::{CliLine, EstateSession, SatzBinary};
use satz_studio_core::settings::Settings;

pub use ansi::strip_ansi;
pub use app_actions::{AppAction, app_coroutine};
pub use estate_actions::{EstateAction, command_line, estate_coroutine};
pub use toast::{Toast, ToastKind, dismiss, enqueue};

/// Where the satz binary stands, as the app coroutine found it at startup and after
/// every settings save.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SatzStatus {
    #[default]
    Unknown,
    Located(SatzBinary),
    TooOld {
        found: String,
        required: String,
    },
    Missing(String),
}

impl SatzStatus {
    pub fn binary(&self) -> Option<&SatzBinary> {
        match self {
            SatzStatus::Located(b) => Some(b),
            _ => None,
        }
    }
}

/// The destinations of the navigation rail, in rail order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Estates,
    Interview,
    Params,
    Map,
    Resources,
    Commands,
    Chat,
    Settings,
    Gallery,
}

impl View {
    pub const ALL: [View; 9] = [
        View::Estates,
        View::Interview,
        View::Params,
        View::Map,
        View::Resources,
        View::Commands,
        View::Chat,
        View::Settings,
        View::Gallery,
    ];

    pub fn label(self) -> &'static str {
        match self {
            View::Estates => "Estates",
            View::Interview => "Interview",
            View::Params => "Params",
            View::Map => "Map",
            View::Resources => "Resources",
            View::Commands => "Commands",
            View::Chat => "Chat",
            View::Settings => "Settings",
            View::Gallery => "Gallery",
        }
    }

    /// The Material Symbols ligature of the destination.
    pub fn icon(self) -> &'static str {
        match self {
            View::Estates => "home_storage",
            View::Interview => "quiz",
            View::Params => "tune",
            View::Map => "map",
            View::Resources => "account_tree",
            View::Commands => "terminal",
            View::Chat => "chat",
            View::Settings => "settings",
            View::Gallery => "palette",
        }
    }

    /// A destination that works on the open estate and shows a card without one.
    pub fn needs_estate(self) -> bool {
        matches!(
            self,
            View::Interview
                | View::Params
                | View::Map
                | View::Resources
                | View::Commands
                | View::Chat
        )
    }
}

/// One `.satz` estate file found beside a `config.toml`.
#[derive(Debug, Clone, PartialEq)]
pub struct EstateFile {
    pub path: PathBuf,
    /// the file name
    pub name: String,
    /// `local`, `cloud`, `None` when the estate binds nothing, or the parse error
    pub deployment_mode: Result<Option<String>, String>,
}

/// One `config.toml` the Estates view found, with the estates beside it.
#[derive(Debug, Clone, PartialEq)]
pub struct EstateSummary {
    pub config: PathBuf,
    pub dir: PathBuf,
    pub estates: Vec<EstateFile>,
    /// the config could not be opened or its `yaml_dir` not read
    pub error: Option<String>,
}

/// The estate a session is open on.
#[derive(Clone)]
pub struct OpenEstate {
    pub session: Arc<EstateSession>,
    pub dir: PathBuf,
    pub main: PathBuf,
    /// the main file's name
    pub name: String,
    pub runs_as: Option<String>,
    pub deployment_mode: Option<String>,
}

impl PartialEq for OpenEstate {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.session, &other.session)
    }
}

impl std::fmt::Debug for OpenEstate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenEstate")
            .field("main", &self.main)
            .finish()
    }
}

/// What `Credential::resolve` answered, for the Settings view.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CredentialStatus {
    #[default]
    Unknown,
    Resolved(CredentialSource),
    Error(String),
}

/// Everything about the open estate that the views read. Reset when an estate opens
/// and when it closes; written by the estate coroutine only.
#[derive(Store, Default)]
pub struct EstateStore {
    pub model: Option<Arc<EstateModel>>,
    pub questions: Option<QuestionsReport>,
    pub diagnostics: Vec<Diagnostic>,
    /// the streamed output of the last command, ANSI stripped
    pub command_log: Vec<CliLine>,
    /// a command is running: Run is disabled, Cancel is enabled
    pub running: bool,
    /// the command line of the running or last command, for the log header
    pub last_command: Option<String>,
    /// how the last command or tool call ended
    pub outcome: Option<CommandOutcome>,
    /// the estate is being reloaded (questions, parse, model)
    pub loading: bool,
}

/// How a command or a tool call ended, shown under the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutcome {
    pub ok: bool,
    pub text: String,
}

#[derive(Store)]
pub struct AppStore {
    pub settings: Settings,
    pub satz: SatzStatus,
    /// the folder the Estates view walks
    pub root: Option<PathBuf>,
    pub estates: Vec<EstateSummary>,
    /// a walk is in progress
    pub discovering: bool,
    pub open: Option<OpenEstate>,
    /// the estate file a session is being opened on
    pub opening: Option<PathBuf>,
    pub nav: View,
    pub snackbar: VecDeque<Toast>,
    pub credential: CredentialStatus,
    pub drawer_open: bool,
    pub estate: EstateStore,
}

impl AppStore {
    pub fn new(settings: Settings) -> Self {
        let root = settings.last_root.clone();
        Self {
            settings,
            satz: SatzStatus::Unknown,
            root,
            estates: Vec::new(),
            discovering: false,
            open: None,
            opening: None,
            nav: View::Estates,
            snackbar: VecDeque::new(),
            credential: CredentialStatus::Unknown,
            drawer_open: false,
            estate: EstateStore::default(),
        }
    }
}

/// The diagnostic the drawer selected last; the estate views scroll to it.
#[derive(Clone, Copy, PartialEq)]
pub struct DiagnosticSelection(pub Signal<Option<Diagnostic>>);

/// Push a toast and schedule its removal. Errors stay longer than notices.
pub fn toast(app: Store<AppStore>, kind: ToastKind, text: impl Into<String>) {
    let id = enqueue(&mut app.snackbar().write(), kind, text.into());
    let after = match kind {
        ToastKind::Info => std::time::Duration::from_secs(5),
        ToastKind::Error => std::time::Duration::from_secs(12),
    };
    spawn(async move {
        tokio::time::sleep(after).await;
        dismiss(&mut app.snackbar().write(), id);
    });
}
