//! The stores and the two coroutines. [`AppStore`] is the whole application state,
//! provided at the root and read through the accessors `#[derive(Store)]` generates, so
//! a log line does not re-render the sidebar. Every side effect — locating satz, walking a
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
use dioxus_icons::lucide::{
    Bot, Boxes, FileText, FolderOpen, HardDrive, HousePlus, Inbox, LayoutDashboard,
    MessageCircleQuestionMark, Package, Rocket, ScrollText, Shield,
};
use satz_studio_core::cst::Cst;
use satz_studio_core::diag::{DiagSource, Diagnostic};
use satz_studio_core::estate::HclState;
use satz_studio_core::git::WorkTree;
use satz_studio_core::github::StudioUpdate;
use satz_studio_core::model::EstateModel;
use satz_studio_core::satz::reports::{
    InterfacesReport, InterviewReport, NoticeRow, PackSourcesReport, PrerequisitesReport,
    QuestionsReport, UpdatePackReport, WorkloadPacksReport,
};
use satz_studio_core::satz::review::ReviewedPack;
use satz_studio_core::satz::self_update::SatzRelease;
use satz_studio_core::satz::{CliLine, EstateSession, ImportReport, QuestionsFormat, SatzBinary};
use satz_studio_core::settings::Settings;

pub use ansi::strip_ansi;
pub use app_actions::{AppAction, app_coroutine, run_line};
pub use estate_actions::{
    EstateAction, PackOp, SourceOp, command_line, edit_file, estate_coroutine, quote, reports_dir,
};
pub use pace::{
    ahead_sentence, install_offer, newer_satz_sentence, satz_available, satz_notice,
    satz_release_sentence, studio_available, studio_look_sentence, window_title,
};
pub use toast::{Toast, ToastKind};

/// Where the satz binary stands, as the app coroutine found it at startup and after
/// every settings save.
///
/// `binary()` answers for [`SatzStatus::Located`] alone, and every way an estate is
/// opened, created or imported asks it first. A satz NEWER than the build is
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

/// Where the window can stand. The sidebar's order is the order of the work on an estate:
/// which estate it is and what it still has to do, which packs it runs, what those packs
/// leave undecided, what it declares, what judges it, what hands it off — then the two
/// secondary destinations at the foot of the sidebar.
///
/// [`View::Start`] is not a sidebar destination: it is the screen with the three doors,
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
    /// the preset library's pack lines: which are in, which are off, which the file has
    /// no line for. The destination of an estate that holds the library — an
    /// organization's; a workload estate has none
    Presets,
    /// the estate's workload packs: templates taken from a source and adapted here
    Packs,
    /// the estate file itself: its params and its resource tree
    Estate,
    /// the estate's way to the organization: who is signed in, what state it is in, and
    /// the steps — the checks, the bootstrap, the HCL code, the plan, the apply
    Apply,
    /// which security checks the estate's own resources fail, as hints: `satz scan`,
    /// Checkov over the HCL code the last transpile wrote (ADR 0035)
    Security,
    /// what the estate hands to people as a file: the decisions sheet
    Documents,
    /// setting an external agent up on this estate, and starting it
    Agent,
    Settings,
}

impl View {
    /// The primary destinations, in sidebar order: the order the work happens, which runs
    /// from what the estate HAS to what it still has to decide. A pack is what declares a
    /// question, and `merge-presets` — the command that brings new pack lines in — is in
    /// Packs, so Packs is where the questions in Decisions come from and it stands before
    /// them. Apply is the estate's way to the organization, Security reads what the
    /// transpile Apply runs wrote, and Documents is what is given to people once the rest
    /// is settled. Those are the path, each ending with a bar that leads to the next
    /// (`views/path.rs`); Estate stands after them, because the estate file itself is not
    /// a step of the walk (ADR 0027, ADR 0028, ADR 0035).
    pub const PRIMARY: [View; 8] = [
        View::Overview,
        View::Presets,
        View::Packs,
        View::Decisions,
        View::Apply,
        View::Security,
        View::Documents,
        View::Estate,
    ];

    /// The primary destinations of the open estate. An estate that holds no preset
    /// library, as satz's presets report says ([`PacksReport::has_presets`]) — a workload
    /// estate without one of its own — has no Presets destination: the library is the
    /// organization's estate's.
    pub fn primary(presets: bool) -> Vec<View> {
        View::PRIMARY
            .into_iter()
            .filter(|v| presets || *v != View::Presets)
            .collect()
    }

    /// The secondary group, in the foot of the sidebar. Agent needs an estate — it
    /// configures an agent for the one that is open; Settings is the one destination
    /// that stands without one.
    pub const SECONDARY: [View; 2] = [View::Agent, View::Settings];

    pub fn label(self) -> &'static str {
        match self {
            View::Start => "Estates",
            View::Overview => "Overview",
            View::Decisions => "Decisions",
            View::Presets => "Presets",
            View::Packs => "Packs",
            View::Estate => "Estate",
            View::Apply => "Apply",
            View::Security => "Security",
            View::Documents => "Documents",
            View::Agent => "Agent",
            View::Settings => "Settings",
        }
    }

    /// The Lucide icon of the destination, `size` pixels square.
    pub fn icon(self, size: u32) -> Element {
        match self {
            View::Start => rsx! { HardDrive { size } },
            View::Overview => rsx! { LayoutDashboard { size } },
            View::Decisions => rsx! { MessageCircleQuestionMark { size } },
            View::Presets => rsx! { Boxes { size } },
            View::Packs => rsx! { Package { size } },
            View::Estate => rsx! { FileText { size } },
            View::Apply => rsx! { Rocket { size } },
            View::Security => rsx! { Shield { size } },
            View::Documents => rsx! { ScrollText { size } },
            View::Agent => rsx! { Bot { size } },
            View::Settings => rsx! { dioxus_icons::lucide::Settings { size } },
        }
    }

    /// A destination that works on the open estate and shows a card without one.
    pub fn needs_estate(self) -> bool {
        matches!(
            self,
            View::Overview
                | View::Decisions
                | View::Presets
                | View::Packs
                | View::Estate
                | View::Apply
                | View::Security
                | View::Documents
                | View::Agent
        )
    }
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

    /// The Lucide icon of the door, `size` pixels square.
    pub fn icon(self, size: u32) -> Element {
        match self {
            Door::Create => rsx! { HousePlus { size } },
            Door::Import => rsx! { Inbox { size } },
            Door::Open => rsx! { FolderOpen { size } },
        }
    }

    /// The line under the door's name: what comes through it.
    pub fn supporting(self) -> &'static str {
        match self {
            Door::Create => {
                "A new estate, made by satz init: the config, the directories and the estate file, with what your sign-in answers already in it."
            }
            Door::Import => {
                "An estate made by satz import out of what exists: a tofu show -json document, a live organization, folder or project, Terraform HCL, or a legacy YAML file."
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
    /// the estate is being reloaded (questions, parse, model, the transpile's own check)
    pub loading: bool,
    /// what the HCL directory holds, read at every reload: the Overview says
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
    /// the pack the Presets view reviewed last with `satz review-preset`, or why its review
    /// failed; its findings stand in the drawer beside the estate's own until it is closed
    pub review: Option<PackReviewState>,
    /// a review or a placement is running
    pub reviewing: bool,
    /// what the estate publishes, from `satz interfaces` at every reload — or satz's
    /// reason it could not say; `None` until the first reload has asked
    pub interfaces: Option<Result<InterfacesReport, String>>,
    /// the estate's pack sources as `satz pack-source list` names them — or satz's reason
    /// it could not; read when the Packs view first stands, and again after a source is
    /// added or removed
    pub pack_sources: Option<Result<PackSourcesReport, String>>,
    /// what satz said to the last `satz pack-source add` or `remove`: `Ok` with the
    /// source's name when it was written, `Err` with satz's refusal; `None` while one runs
    /// and before the first
    pub source_change: Option<Result<String, String>>,
    /// the estate's workload packs as `satz packs` listed them against
    /// [`Self::pack_source`] — or satz's reason it could not; `None` until a source has
    /// been chosen, which happens only when the operator asks: a GitHub source is a
    /// network read
    pub workload_packs: Option<Result<WorkloadPacksReport, String>>,
    /// the packs of `packs_dir` as `satz packs --local` listed them — no pack source read,
    /// so no network — with the interface folders the estate uses; read when Packs first
    /// stands and after every pack command; `None` until then
    pub local_packs: Option<Result<WorkloadPacksReport, String>>,
    /// the name of the pack source the list was read from, and the one a fetch reads
    pub pack_source: Option<String>,
    /// what the last `satz update-pack` reported: the edited packs with their
    /// difference stand in the tab until the next list
    pub pack_update: Option<UpdatePackReport>,
    /// what satz said to the last `satz publish-pack`: its report when the pack was
    /// published, its refusal as it said it; `None` while one runs and before the first
    pub pack_publish: Option<Result<satz_studio_core::satz::reports::Published, String>>,
    /// a list, a fetch, an update, a detach or a publish is running
    pub packs_busy: bool,
    /// who is signed in on this machine and what the estate's live commands run as, as
    /// `satz_whoami` answered — or why it could not; `None` until it has been asked,
    /// which the Apply destination does: the check mints a token, a network call
    pub sign_in: Option<SignIn>,
    /// what a workload estate's resource types need from the organization's team, as
    /// `satz_update_prerequisites {report_only: true}` lists it — or why it could not;
    /// `None` until the Overview's "Needed roles" asks, and while it is read again
    pub needed: Option<Result<PrerequisitesReport, String>>,
    /// the last `satz scan` of this estate: Checkov's failed checks on the estate's own
    /// resources, or satz's refusal in its words; `None` until the Security destination's
    /// "Scan" asks — Checkov runs for a while, so it never runs unasked
    pub scan: Option<Result<satz_studio_core::satz::reports::ScanReport, String>>,
    /// a scan is running
    pub scanning: bool,
}

/// The sign-in check of the Apply destination.
#[derive(Debug, Clone, PartialEq)]
pub enum SignIn {
    Checking,
    /// satz's report: the sign-in, the estate's account and what was checked
    Known(Box<satz_studio_core::satz::reports::WhoamiReport>),
    /// satz's refusal, in its own words — no sign-in, or a token that is not minted
    Refused(String),
}

/// The pack review of the Presets view.
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

    /// The review's findings, for the drawer; none for a failed review,
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
/// `init` derives a customer's organization id, billing account and administrator
/// address from the sign-in and prints where each came from. Those lines are held
/// here, in memory, for as long as the window shows them; they are written into the
/// estate satz created and nowhere else — not into `Settings`, not into a file of this
/// app's own.
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
/// it matter more: the run prints the organization id, the customer directory id, the
/// billing account and an administrator's address it derived from the sign-in. Those
/// lines live here, in memory, for as long as the window shows them; they are written
/// into the estate satz created and nowhere else — not into `Settings`, not into a file
/// of this app's own.
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
    /// the palette opens on the pack review rather than its first command: a finding of
    /// the reviewed pack was chosen in the drawer
    pub palette_review: bool,
    /// what waits to be shown as a toast
    pub toasts: VecDeque<Toast>,
    /// every error the window reported, in full, newest last and at most
    /// [`ERRORS_KEPT`]: an error's toast is only the count, and the Findings drawer lists
    /// these until they are cleared (ADR 0036)
    pub errors: Vec<Diagnostic>,
    pub drawer_open: bool,
    pub estate: EstateStore,
    /// the `satz init` run behind the Create door
    pub create: CreateStore,
    /// the `satz import` run behind the Import door
    pub import: ImportStore,
    /// the `satz self-update` run offered by the banner and Settings, and the
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
            palette_review: false,
            toasts: VecDeque::new(),
            errors: Vec::new(),
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

/// The finding the drawer selected last; the estate views scroll to it.
#[derive(Clone, Copy, PartialEq)]
pub struct DiagnosticSelection(pub Signal<Option<Diagnostic>>);

/// Hand the window one thing to say; it shows it as a toast. An error is also kept in
/// full for the Findings drawer, since its toast says only how many came (ADR 0036).
pub fn toast(app: Store<AppStore>, kind: ToastKind, text: impl Into<String>) {
    let text = text.into();
    if kind == ToastKind::Error {
        keep_error(&mut app.errors().write(), text.clone());
    }
    app.toasts().write().push_back(Toast { kind, text });
}

/// An error toast for an error the Findings drawer lists as a finding of the estate
/// already — a reload's check, a write satz refused: counted in the toast and not kept a
/// second time, since the next reload is what updates the finding (ADR 0036).
pub fn toast_finding(app: Store<AppStore>, kind: ToastKind, text: impl Into<String>) {
    app.toasts().write().push_back(Toast {
        kind,
        text: text.into(),
    });
}

/// How many of the window's errors the drawer keeps; the oldest goes first.
pub const ERRORS_KEPT: usize = 50;

/// `text` added to the window's errors as a finding of its own, the oldest dropped past
/// [`ERRORS_KEPT`].
pub fn keep_error(errors: &mut Vec<Diagnostic>, text: String) {
    errors.push(Diagnostic::error(text, DiagSource::Window));
    let over = errors.len().saturating_sub(ERRORS_KEPT);
    errors.drain(..over);
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The Start screen is where the window stands with no estate open, not a
    /// destination of the sidebar.
    #[test]
    fn the_start_screen_is_in_neither_group() {
        assert!(!View::PRIMARY.contains(&View::Start));
        assert!(!View::SECONDARY.contains(&View::Start));
    }

    #[test]
    fn no_two_destinations_share_a_label() {
        let all = [
            View::Start,
            View::Overview,
            View::Decisions,
            View::Presets,
            View::Packs,
            View::Estate,
            View::Apply,
            View::Security,
            View::Documents,
            View::Agent,
            View::Settings,
        ];
        let mut labels: Vec<&str> = all.iter().map(|v| v.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), all.len());
    }

    /// An organization's estate has Presets and Packs; an estate without a preset library
    /// has every other destination, Packs among them.
    #[test]
    fn an_estate_without_presets_has_every_primary_destination_but_presets() {
        assert_eq!(View::primary(true), View::PRIMARY);
        let workload = View::primary(false);
        assert!(!workload.contains(&View::Presets) && workload.contains(&View::Packs));
        assert_eq!(workload.len(), View::PRIMARY.len() - 1);
        assert_eq!(
            (View::Presets.label(), View::Packs.label()),
            ("Presets", "Packs")
        );
    }
}
