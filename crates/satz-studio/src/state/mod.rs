//! The stores and the two coroutines. [`AppStore`] is the whole application state,
//! provided at the root and read through the accessors `#[derive(Store)]` generates, so
//! a log line does not re-render the rail. Every side effect — locating satz, walking a
//! folder, opening a session, running a command — happens in [`app_coroutine`] or in
//! the per-estate coroutine ([`estate_coroutine`]); the stores are written from there,
//! and nothing blocks in an event handler.

mod ansi;
mod app_actions;
mod estate_actions;
mod install;
mod pace;
mod toast;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dioxus::prelude::*;
use satz_studio_core::cst::Cst;
use satz_studio_core::diag::Diagnostic;
use satz_studio_core::estate::HclState;
use satz_studio_core::git::WorkTree;
use satz_studio_core::github::StudioUpdate;
use satz_studio_core::llm::CredentialSource;
use satz_studio_core::model::EstateModel;
use satz_studio_core::satz::reports::{InterviewReport, NoticeRow, QuestionsReport};
use satz_studio_core::satz::review::ReviewedPack;
use satz_studio_core::satz::self_update::SatzRelease;
use satz_studio_core::satz::{CliLine, EstateSession, ImportReport, QuestionsFormat, SatzBinary};
use satz_studio_core::settings::Settings;

pub use ansi::strip_ansi;
pub use app_actions::{AppAction, app_coroutine, run_line, save_settings};
pub use estate_actions::{EstateAction, command_line, estate_coroutine, quote, reports_dir};
pub use pace::{
    ahead_sentence, install_offer, newer_satz_sentence, satz_available, satz_notice,
    satz_release_sentence, studio_available, studio_look_sentence, window_title,
};
pub use toast::{Toast, ToastKind, dismiss, enqueue};

/// Where the satz binary stands, as the app coroutine found it at startup and after
/// every settings save.
///
/// `binary()` answers for [`SatzStatus::Located`] alone, and every way an estate is
/// opened, created, imported or chatted with asks it first. A satz NEWER than the build is
/// `Located` too: the app copes with it and [`satz_notice`] tells the operator, and the one
/// version it refuses is an older one (ADR 0014).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SatzStatus {
    #[default]
    Unknown,
    Located(SatzBinary),
    TooOld {
        /// the binary that was refused — `satz self-update` is run on this one
        path: PathBuf,
        found: String,
        required: String,
    },
    /// no satz anywhere the search looks
    Missing(String),
    /// a satz that exists and does not run, or prints no version
    Unusable(String),
}

impl SatzStatus {
    pub fn binary(&self) -> Option<&SatzBinary> {
        match self {
            SatzStatus::Located(b) => Some(b),
            _ => None,
        }
    }

    /// The satz that `self-update` would be run on. A binary that is too old is still a
    /// binary that can update itself — that is the whole point of offering it — so this
    /// answers for `TooOld` as well as `Located`, and for nothing else.
    pub fn updatable(&self) -> Option<&Path> {
        match self {
            SatzStatus::Located(b) => Some(&b.path),
            SatzStatus::TooOld { path, .. } => Some(path),
            SatzStatus::Unknown | SatzStatus::Missing(_) | SatzStatus::Unusable(_) => None,
        }
    }
}

/// Where the window can stand. The rail's order is the order of the work on an estate:
/// which estate it is and what it still has to do, which packs it runs, what those packs
/// leave undecided, what it declares, what judges it, what hands it off — then the two
/// secondary destinations at the foot of the rail.
///
/// [`View::Start`] is not a rail destination: it is the screen with the three doors,
/// where the window stands while no estate is open. Switching estates is the top bar's
/// action, not a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Start,
    /// which estate this is, in its own answers, and what it still has to do — both
    /// derived from its own state
    Overview,
    /// the questions its packs declare and it has not answered
    Decisions,
    /// the pack lines: which are in, which are off, which the file has no line for
    Packs,
    /// the estate file itself: its params and its resource tree
    Estate,
    /// what judges the estate: the compile, the goal view, the evidence report
    Checks,
    /// what hands it off: the HCL directory, the plan, the apply, the state migration
    Deploy,
    Chat,
    Settings,
    /// a development route, reachable only with `SATZ_STUDIO_DEBUG` set
    Gallery,
}

impl View {
    /// The primary destinations, in rail order: the order the work happens, which runs
    /// from what the estate HAS to what it still has to decide. A pack is what declares a
    /// question, and `merge-presets` — the command that brings new pack lines in — is in
    /// Packs, so Packs is where the questions in Decisions come from and it stands before
    /// them. Six, and Material 3 allows three to seven — `docs/ui.md` says what a seventh
    /// would cost.
    pub const PRIMARY: [View; 6] = [
        View::Overview,
        View::Packs,
        View::Decisions,
        View::Estate,
        View::Checks,
        View::Deploy,
    ];

    /// The secondary group, bottom-aligned in the rail. Chat needs an estate; Settings
    /// is the one destination that stands without one.
    pub const SECONDARY: [View; 2] = [View::Chat, View::Settings];

    pub fn label(self) -> &'static str {
        match self {
            View::Start => "Estates",
            View::Overview => "Overview",
            View::Decisions => "Decisions",
            View::Packs => "Packs",
            View::Estate => "Estate",
            View::Checks => "Checks",
            View::Deploy => "Deploy",
            View::Chat => "Chat",
            View::Settings => "Settings",
            View::Gallery => "Gallery",
        }
    }

    /// The Material Symbols ligature of the destination.
    pub fn icon(self) -> &'static str {
        match self {
            View::Start => "home_storage",
            View::Overview => "dashboard",
            View::Decisions => "quiz",
            View::Packs => "inventory_2",
            View::Estate => "description",
            View::Checks => "fact_check",
            View::Deploy => "rocket_launch",
            View::Chat => "chat",
            View::Settings => "settings",
            View::Gallery => "palette",
        }
    }

    /// A destination that works on the open estate and shows a card without one.
    pub fn needs_estate(self) -> bool {
        matches!(
            self,
            View::Overview
                | View::Decisions
                | View::Packs
                | View::Estate
                | View::Checks
                | View::Deploy
                | View::Chat
        )
    }
}

/// The development routes — the Gallery — are behind `SATZ_STUDIO_DEBUG`: the component
/// checklist is for whoever changes the components, not for an operator's rail.
pub fn debug_routes() -> bool {
    std::env::var_os("SATZ_STUDIO_DEBUG").is_some_and(|v| !v.is_empty())
}

/// The ways an estate reaches the app: the doors of the Start screen. Each is one card
/// in its door row and one pane below it.
///
/// The row reads from nothing to an estate: **Create** makes one where there is none,
/// **Import** makes one out of infrastructure that already exists in another form, and
/// **Open** takes one that is already written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Door {
    /// `satz init` in a folder that holds no estate yet
    Create,
    /// `satz import` over a state file, a live scope, Terraform HCL or a legacy YAML
    /// file — with `satz init` first when the folder is not an estate yet
    Import,
    /// a folder walked for the estates already in it
    #[default]
    Open,
}

impl Door {
    pub const ALL: [Door; 3] = [Door::Create, Door::Import, Door::Open];

    pub fn label(self) -> &'static str {
        match self {
            Door::Create => "Create",
            Door::Import => "Import",
            Door::Open => "Open",
        }
    }

    /// The Material Symbols ligature of the door.
    pub fn icon(self) -> &'static str {
        match self {
            Door::Create => "add_home",
            Door::Import => "move_to_inbox",
            Door::Open => "folder_open",
        }
    }

    /// The line under the door's name: what comes through it.
    pub fn supporting(self) -> &'static str {
        match self {
            Door::Create => {
                "A new estate, made by satz init: the config, the directories and the estate file, with what your credentials answer already in it."
            }
            Door::Import => {
                "An estate made by satz import out of what exists: a tofu show -json document, a live organisation, folder or project, Terraform HCL, or a legacy YAML file."
            }
            Door::Open => "An estate that is already on disk: choose the folder that holds it.",
        }
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

/// One `config.toml` the Start screen found, with the estates beside it.
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
    /// the main file's document tree as read at the last reload: the views slice a
    /// value's source text and a line's text from it
    pub cst: Option<Arc<Cst>>,
    pub questions: Option<QuestionsReport>,
    /// what the last `satz_interview` call returned — `rename_to` is read from it
    pub interview: Option<InterviewReport>,
    /// the notices a write of this session opened — a pack switched on names a command
    /// to run — and that the estate has not acknowledged since. satz returns each one
    /// once, when it opens, so they are kept here until their param is bound
    pub notices: Vec<NoticeRow>,
    /// the notice dialog stands over the window: raised when notices arrive, lowered by
    /// Later, raised again from the Overview's row
    pub notices_open: bool,
    pub diagnostics: Vec<Diagnostic>,
    /// the streamed output of the last command, ANSI stripped
    pub command_log: Vec<CliLine>,
    /// a command is running: Run is disabled, Cancel is enabled
    pub running: bool,
    /// the command line of the running or last command, for the log header
    pub last_command: Option<String>,
    /// how the last command or tool call ended
    pub outcome: Option<CommandOutcome>,
    /// the estate is being reloaded (questions, parse, model, the compile's own check)
    pub loading: bool,
    /// what the generated HCL directory holds, read at every reload: the Overview says
    /// what is still owed from it
    pub hcl: HclState,
    /// whether git holds the estate file's directory in a work tree, asked at every
    /// reload and after `InitRepository`; `None` until it has been asked
    pub work_tree: Option<WorkTree>,
    /// the formats `satz questions` offers, read from the installed satz's help when the
    /// session opens — or why they could not be read; `None` until then
    pub export_formats: Option<Result<Vec<QuestionsFormat>, String>>,
    /// the last decisions sheet or workbook this session wrote, which "Export again"
    /// writes over
    pub last_export: Option<Exported>,
    /// the pack the Packs view reviewed last with `satz review-pack`, or why its review
    /// failed; its findings stand in the drawer beside the estate's own until it is closed
    pub review: Option<PackReviewState>,
    /// a review or a placement is running
    pub reviewing: bool,
}

/// The pack review of the Packs view.
#[derive(Debug, Clone, PartialEq)]
pub enum PackReviewState {
    Reviewed(ReviewedPack),
    Failed { pack: PathBuf, error: String },
}

impl PackReviewState {
    pub fn pack(&self) -> &Path {
        match self {
            PackReviewState::Reviewed(r) => &r.path,
            PackReviewState::Failed { pack, .. } => pack,
        }
    }

    /// The review's findings as diagnostics, for the drawer; none for a failed review,
    /// whose reason the view and the toast carry.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        match self {
            PackReviewState::Reviewed(r) => r.diagnostics(),
            PackReviewState::Failed { .. } => Vec::new(),
        }
    }
}

/// A document an export wrote: the format and the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exported {
    pub format: String,
    pub path: PathBuf,
}

/// How a command or a tool call ended, shown under the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutcome {
    pub ok: bool,
    pub text: String,
}

/// One `satz init` run behind the Create door, reset when a run starts.
///
/// `init` derives a customer's organisation id, billing account and administrator
/// address from the credentials and prints where each came from. Those lines are held
/// here, in memory, for as long as the window shows them; they are written into the
/// estate satz created and nowhere else — not into `Settings`, not into a transcript,
/// not into a file of this app's own.
#[derive(Store, Default)]
pub struct CreateStore {
    /// the streamed output of the run, ANSI stripped
    pub log: Vec<CliLine>,
    /// a run is in progress: Create is disabled, Cancel is enabled
    pub running: bool,
    /// the command line of the running or last run, for the log header
    pub command: Option<String>,
    /// how the last run ended
    pub outcome: Option<CommandOutcome>,
}

/// The `satz import` run behind the Import door, reset when a run starts.
///
/// Same four fields as [`CreateStore`] — it is the same kind of thing, one streamed run
/// with an outcome — plus the report satz printed, which is the whole point of the run:
/// what it wrote, what it could not derive and what it left out.
///
/// The privacy rule of [`CreateStore`] holds here word for word, and a live import makes
/// it matter more: the run prints the organisation id, the customer directory id, the
/// billing account and an administrator's address it derived from the credentials. Those
/// lines live here, in memory, for as long as the window shows them; they are written
/// into the estate satz created and nowhere else — not into `Settings`, not into a
/// transcript, not into a file of this app's own.
#[derive(Store, Default)]
pub struct ImportStore {
    /// the streamed output of the run, ANSI stripped — `satz init` too, on the two-step
    /// path, so the log is the whole sequence
    pub log: Vec<CliLine>,
    /// a run is in progress: Import is disabled, Cancel is enabled
    pub running: bool,
    /// the command line, or the two of them, of the running or last run
    pub command: Option<String>,
    /// how the last run ended
    pub outcome: Option<CommandOutcome>,
    /// satz's own import report, split out of what the import run printed
    pub report: ImportReport,
}

/// The `satz self-update` run: satz owns its own updater, so the app only runs it and
/// shows what it said. Same shape as [`CreateStore`], because it is the same kind of
/// thing — one streamed command with an outcome.
#[derive(Store, Default)]
pub struct UpdateStore {
    /// the streamed output of the run, ANSI stripped
    pub log: Vec<CliLine>,
    /// a run is in progress
    pub running: bool,
    /// the command line of the running or last run, for the log header
    pub command: Option<String>,
    /// how the last run ended
    pub outcome: Option<CommandOutcome>,
    /// what the last `--check-only` run found — the one the app runs once at launch, or one
    /// the operator asked for — kept for the session and cleared when an update installs
    pub found: Option<Result<SatzRelease, String>>,
    /// why no check ran at launch, when none did: the operator's satz config says
    /// `self_update_frequency = "never"`, or could not be read
    pub not_checked: Option<String>,
}

/// The look for a newer satz-studio: once at launch, and again when asked. A look reads
/// the latest release on GitHub and compares it with this build; it downloads nothing,
/// runs nothing and writes nothing, and its result is kept for the session.
#[derive(Store, Default)]
pub struct StudioLookStore {
    /// a look is in flight
    pub looking: bool,
    /// what the last look found, or why it failed, as the sentence to show
    pub outcome: Option<Result<StudioUpdate, String>>,
}

/// The run of satz's own installer, offered while no satz is found. Same shape as
/// [`UpdateStore`]: one streamed run with an outcome — here preceded by the download and
/// the check against the SHA-256 sidecar, whose lines lead the log.
#[derive(Store, Default)]
pub struct InstallStore {
    /// the streamed output of the run, ANSI stripped
    pub log: Vec<CliLine>,
    /// a run is in progress
    pub running: bool,
    /// what is being run, for the log header
    pub command: Option<String>,
    /// how the last run ended
    pub outcome: Option<CommandOutcome>,
}

#[derive(Store)]
pub struct AppStore {
    pub settings: Settings,
    pub satz: SatzStatus,
    /// the folder the Start screen walks
    pub root: Option<PathBuf>,
    pub estates: Vec<EstateSummary>,
    /// a walk is in progress
    pub discovering: bool,
    pub open: Option<OpenEstate>,
    /// the estate file a session is being opened on
    pub opening: Option<PathBuf>,
    pub nav: View,
    /// the door of the Start screen the pane below the row belongs to
    pub door: Door,
    /// the command palette is over the window
    pub palette_open: bool,
    pub snackbar: VecDeque<Toast>,
    pub credential: CredentialStatus,
    pub drawer_open: bool,
    pub estate: EstateStore,
    /// the `satz init` run behind the Create door
    pub create: CreateStore,
    /// the `satz import` run behind the Import door
    pub import: ImportStore,
    /// the `satz self-update` run offered by the banner, the top bar and Settings, and the
    /// `--check-only` run the app makes once at launch
    pub update: UpdateStore,
    /// the look for a newer satz-studio: once at launch, again from Settings
    pub studio_look: StudioLookStore,
    /// satz's installer, offered by the banner and by Settings while no satz is found
    pub install: InstallStore,
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
            nav: View::default(),
            door: Door::default(),
            palette_open: false,
            snackbar: VecDeque::new(),
            credential: CredentialStatus::Unknown,
            drawer_open: false,
            estate: EstateStore::default(),
            create: CreateStore::default(),
            import: ImportStore::default(),
            update: UpdateStore::default(),
            studio_look: StudioLookStore::default(),
            install: InstallStore::default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Material 3 puts three to seven destinations in a navigation rail. Six primary
    /// ones and a bottom-aligned group of two is inside the pattern because the group
    /// is secondary; a seventh PRIMARY destination is not, and the answer then is a
    /// navigation drawer rather than a smaller font (`docs/ui.md`).
    #[test]
    fn the_rail_stays_inside_the_navigation_rail_pattern() {
        assert!(
            (3..=7).contains(&View::PRIMARY.len()),
            "{} primary destinations",
            View::PRIMARY.len()
        );
        assert_eq!(View::SECONDARY.len(), 2);
    }

    #[test]
    fn every_primary_destination_works_on_an_estate_and_settings_is_the_one_that_does_not() {
        for view in View::PRIMARY {
            assert!(view.needs_estate(), "{view:?}");
        }
        let without: Vec<View> = View::SECONDARY
            .into_iter()
            .filter(|v| !v.needs_estate())
            .collect();
        assert_eq!(without, [View::Settings]);
    }

    /// The Start screen and the Gallery are places the window can stand and not
    /// destinations of the rail: the first is where it stands with no estate open, the
    /// second is a development route.
    #[test]
    fn the_start_screen_and_the_gallery_are_in_neither_group() {
        for view in [View::Start, View::Gallery] {
            assert!(!View::PRIMARY.contains(&view), "{view:?}");
            assert!(!View::SECONDARY.contains(&view), "{view:?}");
        }
    }

    #[test]
    fn no_two_destinations_share_a_label_or_an_icon() {
        let all = [
            View::Start,
            View::Overview,
            View::Decisions,
            View::Packs,
            View::Estate,
            View::Checks,
            View::Deploy,
            View::Chat,
            View::Settings,
            View::Gallery,
        ];
        let mut labels: Vec<&str> = all.iter().map(|v| v.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), all.len());
        let mut icons: Vec<&str> = all.iter().map(|v| v.icon()).collect();
        icons.sort_unstable();
        icons.dedup();
        assert_eq!(icons.len(), all.len());
    }
}
