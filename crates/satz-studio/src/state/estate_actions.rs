//! The per-estate coroutine: one per open session, owning its `Arc<EstateSession>`.
//! It reloads the model, runs CLI commands with their output streamed into the store,
//! calls tools, hands `apply` and `bootstrap` to the OS terminal, and is the one place
//! the estate is written from: an answer through satz's own writer, a value through
//! the app's, a pack switched through `satz_add_pack` and `satz_remove_pack` — each under
//! the session's write lock, each verified by `satz transpile --check`, each followed by
//! a reload. It also runs `satz review-pack` for the Packs view and places a reviewed
//! pack in the estate's library, the estate checked with it there. A running
//! command streams from a tokio task into a local task, so the loop stays free to take
//! `CancelCommand`; the three git commands that put the estate in a repository run the
//! same way, when the operator asks for them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dioxus::prelude::*;
use futures_util::StreamExt;
use satz_studio_core::cst::Cst;
use satz_studio_core::diag::{DiagSource, Diagnostic, Severity};
use satz_studio_core::edit::{
    CheckFailure, Checker, CommitError, Committed, Delegated, Edit, EditSession, McpChecker,
    Rollback, Snapshot,
};
use satz_studio_core::estate::{EstateDir, HclState};
use satz_studio_core::git::{self, WorkTree};
use satz_studio_core::model::EstateModel;
use satz_studio_core::satz::reports::{
    AddPackArgs, FindingSeverity, InterviewArgs, InterviewReport, MergeReport, NoticeRow,
    PackChange, PackReview, PacksReport, PrerequisitesResult, QuestionsReport, RemovePackArgs,
};
use satz_studio_core::satz::review::{self, PlaceError, Placed};
use satz_studio_core::satz::{CliLine, EstateSession, ToolOutcome, export};
use satz_studio_core::schema::{ResourceRegistry, SchemaError};
use tokio_util::sync::CancellationToken;

use super::app_actions::close_estate;
use super::{
    AppStore, AppStoreStoreExt, CommandOutcome, Door, EstateStore, EstateStoreStoreExt, Exported,
    PackReviewState, ToastKind, strip_ansi, toast,
};

pub enum EstateAction {
    /// questions, parse, params, schema, model — after every write and on demand
    Reload,
    /// `satz --config <dir> <args…>`, streamed into the log; a reporting command's
    /// file goes into the log after it, whole
    RunCommand(Vec<String>),
    /// the command a pack's notice names, run the same way — and the estate read again
    /// when it ends, because that command writes the file: `satz adopt --execute
    /// --import` puts the live ids in it and binds the notice's param itself
    RunNoticeCommand(Vec<String>),
    CancelCommand,
    /// `satz questions <estate> --format <format> --out <out>`, run like any other
    /// command: the decisions sheet or the workbook at the path the operator chose,
    /// opened once satz has written it
    Export {
        format: String,
        out: PathBuf,
    },
    /// `apply` or `bootstrap`: a one-shot script, opened in the OS terminal
    OpenInTerminal(Vec<String>),
    /// one answer through satz's own writer, `satz_interview {answers: {subject:
    /// value}}`; for a `oneof` the value is the chosen option's param name
    Answer {
        subject: String,
        value: serde_json::Value,
    },
    /// `satz_interview {accept_defaults: true}`: every default the report offers
    AcceptDefaults,
    /// `satz_update_prerequisites {report_only: false}`: the roles and APIs the
    /// estate's own resource types oblige it to declare, written into the file by
    /// satz's writer under the same discipline as an answer
    WritePrerequisites,
    /// the app's own writer: the edit applied in memory, checked as a temp file
    /// beside the real one, renamed over it
    CommitEdit(Edit),
    /// `satz_add_pack`: the pack's gate bound true and its line made active where the
    /// pack graph places it, by satz's own writer — the map's line as much as any other
    AddPack(AddPackArgs),
    /// `satz_remove_pack`: the pack's gate bound false, its line left as it is
    RemovePack(RemovePackArgs),
    /// `satz_merge_presets`: the line for a pack the library gained
    MergePresets,
    /// `satz --config <dir> review-pack <pack> [--against <estate>]`: the pack judged
    /// against the library's bar, its findings into the drawer at their lines
    ReviewPack {
        pack: PathBuf,
        against: bool,
    },
    /// the reviewed bytes into the estate's `presets_dir` as `<stem>.local.satz`, the
    /// estate checked with it there, then the placed file reviewed where it now stands
    PlacePrivate,
    /// the review closed: its card and its findings leave the window
    CloseReview,
    /// `git init -b main`, `git add -A` and one commit in the estate directory, streamed
    /// into the log: the repository `satz merge-presets` needs for its undo
    InitRepository,
    /// leave the estate: the window has nowhere to stand without one, so it goes back to
    /// the Start screen as it stands
    Close,
    /// leave it to open another: the Start screen again, with the Open door showing, so
    /// the estates it has found are the first thing there
    Switch,
}

pub async fn estate_coroutine(
    mut rx: UnboundedReceiver<EstateAction>,
    session: Arc<EstateSession>,
    app: Store<AppStore>,
) {
    app.estate().set(EstateStore::default());
    reload(&session, app).await;
    read_export_formats(&session, app).await;
    let mut running = RunningCommand::default();
    while let Some(action) = rx.next().await {
        match action {
            EstateAction::Reload => reload(&session, app).await,
            EstateAction::RunCommand(args) => {
                running.started(run_command(&session, app, args, After::Nothing))
            }
            EstateAction::RunNoticeCommand(args) => {
                running.started(run_command(&session, app, args, After::Reload))
            }
            EstateAction::CancelCommand => running.cancel(),
            EstateAction::Export { format, out } => {
                let Some(open) = app.open().cloned() else {
                    toast(app, ToastKind::Error, "no estate is open to export");
                    continue;
                };
                let args = export::args(&open.name, &format, &out);
                running.started(run_command(
                    &session,
                    app,
                    args,
                    After::Open(Exported { format, path: out }),
                ));
            }
            EstateAction::OpenInTerminal(args) => open_in_terminal(&session, app, &args),
            EstateAction::Answer { subject, value } => {
                let args = InterviewArgs {
                    answers: BTreeMap::from([(subject, value)]),
                    ..Default::default()
                };
                interview(&session, app, args).await;
            }
            EstateAction::AcceptDefaults => {
                let args = InterviewArgs {
                    accept_defaults: true,
                    ..Default::default()
                };
                interview(&session, app, args).await;
            }
            EstateAction::WritePrerequisites => write_prerequisites(&session, app).await,
            EstateAction::CommitEdit(edit) => commit_edit(&session, app, edit).await,
            EstateAction::AddPack(args) => switch_pack(&session, app, "satz_add_pack", &args).await,
            EstateAction::RemovePack(args) => {
                switch_pack(&session, app, "satz_remove_pack", &args).await
            }
            EstateAction::MergePresets => {
                {
                    let _lock = session.write_lock().await;
                    merge_presets(&session, app).await;
                }
                reload(&session, app).await;
            }
            EstateAction::ReviewPack { pack, against } => {
                if app.estate().reviewing().cloned() {
                    toast(app, ToastKind::Info, "a review is already running");
                    continue;
                }
                app.estate().reviewing().set(true);
                let session = Arc::clone(&session);
                spawn(async move {
                    review_pack(&session, app, pack, against).await;
                    app.estate().reviewing().set(false);
                });
            }
            EstateAction::PlacePrivate => {
                if app.estate().reviewing().cloned() {
                    toast(app, ToastKind::Info, "a review is already running");
                    continue;
                }
                app.estate().reviewing().set(true);
                place_private(&session, app).await;
                app.estate().reviewing().set(false);
            }
            EstateAction::CloseReview => app.estate().review().set(None),
            EstateAction::InitRepository => running.started(init_repository(&session, app)),
            EstateAction::Close => close_estate(app),
            EstateAction::Switch => {
                close_estate(app);
                app.door().set(Door::Open);
            }
        }
    }
}

/// The cancel token of the command the estate's log is running, which Cancel reaches.
/// A start that was refused — a command already running, a directory that could not be
/// made — hands back no token and leaves the running command's in place, so the command
/// that is running stays cancellable.
#[derive(Debug, Default)]
struct RunningCommand(Option<CancellationToken>);

impl RunningCommand {
    fn started(&mut self, token: Option<CancellationToken>) {
        if let Some(token) = token {
            self.0 = Some(token);
        }
    }

    fn cancel(&mut self) {
        if let Some(token) = self.0.take() {
            token.cancel();
        }
    }
}

/// What a write hands the reload that follows it.
#[derive(Debug, Default)]
struct Carried {
    /// what the check of the write said — the findings of a check that passed, or the
    /// refusal's own — which the reload keeps instead of running the check again
    checked: Vec<Diagnostic>,
    /// a tool that refused: satz's own sentence, in the drawer beside what the reload's
    /// check says of the file it left as it was
    refused: Option<Diagnostic>,
}

impl Carried {
    fn checked(checked: Vec<Diagnostic>) -> Carried {
        Carried {
            checked,
            refused: None,
        }
    }
}

/// A delegated write: satz's own writer works on the real file, so the bytes are
/// recorded first and [`Snapshot::delegate`] runs the call around them — a call that
/// landed is checked on the real path and restored when the check refuses; a refusal, or
/// a call that returned nothing, is compared with the record and restored when satz had
/// changed the file. A call that did not land is satz's own sentence, followed by what
/// became of the file when satz had changed it, in a toast and in the drawer. `landed`
/// reads the outcome of a call that landed and says what the toast says — `Err` for an
/// outcome the app could not type, which is a toast in the error colour over a write that
/// is already on disk.
async fn delegated_write<F>(
    session: &Arc<EstateSession>,
    app: Store<AppStore>,
    name: &str,
    args: serde_json::Map<String, serde_json::Value>,
    landed: F,
) -> Carried
where
    F: FnOnce(&ToolOutcome) -> Result<String, String>,
{
    let _lock = session.write_lock().await;
    let snapshot = match Snapshot::take(&session.main) {
        Ok(s) => s,
        Err(e) => {
            toast(app, ToastKind::Error, e.to_string());
            return Carried::default();
        }
    };
    let checker = McpChecker {
        session: Arc::clone(session),
    };
    match snapshot.delegate(session.tool(name, args), &checker).await {
        Delegated::Landed { outcome, committed } => {
            match landed(&outcome) {
                Ok(text) => toast(app, ToastKind::Info, text),
                Err(e) => toast(app, ToastKind::Error, e),
            }
            Carried::checked(carried_findings(&committed))
        }
        Delegated::RolledBack { error, .. } => Carried::checked(rolled_back(app, error)),
        Delegated::NotLanded(not_landed) => {
            let text = not_landed.message(name);
            toast(app, ToastKind::Error, text.clone());
            Carried {
                checked: Vec::new(),
                refused: Some(Diagnostic::error(text, DiagSource::Tool(name.to_string()))),
            }
        }
    }
}

/// One answer, or every default: `satz_interview` on the real file.
///
/// An answer that switches a pack on opens that pack's notices — the command that pack
/// asks to be run — and satz returns each one once, in the report of the call that opened
/// it. They are held in the store from here; the reload is what takes them away again,
/// when the estate binds their param.
async fn interview(session: &Arc<EstateSession>, app: Store<AppStore>, args: InterviewArgs) {
    let Some(args) = serde_json::to_value(&args)
        .ok()
        .and_then(|v| v.as_object().cloned())
    else {
        toast(
            app,
            ToastKind::Error,
            "satz_interview: the arguments did not serialise to an object",
        );
        return;
    };
    let carried = delegated_write(session, app, "satz_interview", args, |outcome| {
        let report = outcome
            .typed::<InterviewReport>("satz_interview")
            .map_err(|e| format!("satz_interview: {e}"))?;
        let written = report.written;
        let opened = report.notices.len();
        queue_notices(app, &report.notices);
        app.estate().interview().set(Some(report));
        Ok(match (written, opened) {
            (1, 0) => "1 answer written".to_string(),
            (n, 0) => format!("{n} answers written"),
            (1, 1) => "1 answer written · 1 notice opened".to_string(),
            (n, o) => format!("{n} answers written · {o} notices opened"),
        })
    })
    .await;
    reload_with(session, app, carried).await;
}

/// One pack switched on or off by satz's own pack logic: `satz_add_pack` or
/// `satz_remove_pack` on the real file, under the delegated-write discipline. satz binds
/// the gate, writes or uncomments the line, compiles the estate and restores it when the
/// compile refuses; a switch satz refuses — a pack it needs is off, a pack that needs it
/// is on, a line not gated on its gate — wrote nothing and names why. A switch that
/// turns a pack on can open its notices, which the window raises as an answer's do.
async fn switch_pack<A: serde::Serialize>(
    session: &Arc<EstateSession>,
    app: Store<AppStore>,
    tool: &'static str,
    args: &A,
) {
    let Some(args) = serde_json::to_value(args)
        .ok()
        .and_then(|v| v.as_object().cloned())
    else {
        toast(
            app,
            ToastKind::Error,
            format!("{tool}: the arguments did not serialise to an object"),
        );
        return;
    };
    let carried = delegated_write(session, app, tool, args, |outcome| {
        let change = outcome
            .typed::<PackChange>(tool)
            .map_err(|e| format!("{tool}: {e}"))?;
        queue_notices(app, &change.notices);
        Ok(switched(&change))
    })
    .await;
    reload_with(session, app, carried).await;
}

/// What the toast says after a switch landed: the packs it switched, or what satz left
/// as it was when it switched nothing.
pub fn switched(change: &PackChange) -> String {
    let verb = if change.action == "remove" {
        "off"
    } else {
        "on"
    };
    let mut text = match change.switched.as_slice() {
        [] => change
            .left
            .first()
            .cloned()
            .unwrap_or_else(|| "nothing switched".to_string()),
        [one] => format!("{one} {verb}"),
        many => format!("{} packs {verb}", many.len()),
    };
    match change.opened.len() {
        0 => {}
        1 => text.push_str(" · 1 question opened"),
        n => text.push_str(&format!(" · {n} questions opened")),
    }
    match change.notices.len() {
        0 => {}
        1 => text.push_str(" · 1 notice opened"),
        n => text.push_str(&format!(" · {n} notices opened")),
    }
    text
}

/// The notices the window holds after a write: the ones it held, plus the ones this
/// call opened that it does not hold yet. satz reports a notice once — in the report of
/// the call that opened it — so the window keeps it; what takes it away is the estate
/// binding its param, which every reload asks satz about. A notice satz reports as
/// acknowledged is never held: it is the record of a command that has run.
pub fn queued(current: &[NoticeRow], opened: &[NoticeRow]) -> Vec<NoticeRow> {
    let mut out = current.to_vec();
    for n in opened {
        if !n.acknowledged && !out.iter().any(|held| held.param == n.param) {
            out.push(n.clone());
        }
    }
    out
}

/// [`queued`] into the store, with the dialog raised when a notice the window was not
/// holding has opened.
fn queue_notices(app: Store<AppStore>, opened: &[NoticeRow]) {
    let held = app.estate().notices().cloned();
    let next = queued(&held, opened);
    if next.len() > held.len() {
        app.estate().notices_open().set(true);
    }
    app.estate().notices().set(next);
}

/// The writing half of `update-prerequisites`: satz works out which roles the IaC
/// service account lacks and which APIs the infra project does not enable, and writes
/// both into the estate. It is offline and it is satz's own writer, so it goes through
/// the same discipline as an answer — the write lock, the snapshot, the check on the
/// real path — rather than a command that rewrites the file under the window.
async fn write_prerequisites(session: &Arc<EstateSession>, app: Store<AppStore>) {
    let args =
        serde_json::Map::from_iter([("report_only".to_string(), serde_json::Value::Bool(false))]);
    let carried = delegated_write(session, app, "satz_update_prerequisites", args, |outcome| {
        let result = outcome
            .typed::<PrerequisitesResult>("satz_update_prerequisites")
            .map_err(|e| format!("satz_update_prerequisites: {e}"))?;
        Ok(match result.written.len() {
            0 => "nothing was missing".to_string(),
            1 => "1 prerequisite written".to_string(),
            n => format!("{n} prerequisites written"),
        })
    })
    .await;
    reload_with(session, app, carried).await;
}

/// The app's writer: open, apply in memory, commit through the temp file and the
/// check. An edit the document layer refuses wrote nothing; a commit the check
/// refuses leaves the file as it was and its diagnostics in the drawer.
async fn commit_edit(session: &Arc<EstateSession>, app: Store<AppStore>, edit: Edit) {
    let carried = {
        let _lock = session.write_lock().await;
        let main = session.main.clone();
        let proposed =
            tokio::task::spawn_blocking(move || EditSession::open(&main)?.apply(&[edit])).await;
        let proposed = match proposed {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => {
                toast(app, ToastKind::Error, format!("not written: {e}"));
                return;
            }
            Err(e) => {
                toast(
                    app,
                    ToastKind::Error,
                    format!("not written: the edit task failed: {e}"),
                );
                return;
            }
        };
        let checker = McpChecker {
            session: Arc::clone(session),
        };
        match proposed.commit(&checker).await {
            Ok(committed) => {
                let name = committed
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                toast(app, ToastKind::Info, format!("{name} written"));
                carried_findings(&committed)
            }
            Err(e) => rolled_back(app, e),
        }
    };
    reload_with(session, app, Carried::checked(carried)).await;
}

/// The findings of a check that passed, as diagnostics to carry through the reload: a
/// warning the compile raised — a required argument the provider wants, a pack the
/// estate asks for and does not use — belongs in the drawer at its line, beside the
/// write that landed.
fn carried_findings(committed: &Committed) -> Vec<Diagnostic> {
    let base = committed
        .path
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    committed
        .summary
        .findings
        .iter()
        .map(|f| Diagnostic::from_finding(&base, f, DiagSource::Check))
        .collect()
}

/// A commit that did not land, as a toast, and the diagnostics to carry through the
/// reload: a check refusal names its first line and keeps every diagnostic; a file
/// that changed under the edit says so and is reloaded, nothing kept.
fn rolled_back(app: Store<AppStore>, e: CommitError) -> Vec<Diagnostic> {
    match e {
        CommitError::Rollback(Rollback::Check(diags)) => {
            let first = diags
                .iter()
                .find(|d| d.severity == Severity::Error)
                .or(diags.first());
            let text = match first {
                Some(d) => match d.line {
                    Some(line) => format!(
                        "not written — line {line}: {}",
                        d.message.lines().next().unwrap_or_default()
                    ),
                    None => format!(
                        "not written — {}",
                        d.message.lines().next().unwrap_or_default()
                    ),
                },
                None => "not written — satz refused the file".to_string(),
            };
            toast(app, ToastKind::Error, text);
            diags
        }
        CommitError::Rollback(Rollback::ChangedOnDisk) => {
            toast(app, ToastKind::Error, "changed on disk — reloaded");
            Vec::new()
        }
        e => {
            toast(app, ToastKind::Error, format!("not written: {e}"));
            Vec::new()
        }
    }
}

/// Where a reporting command writes when the app names the file. satz's ADR 0021 gives
/// every reporting command one `--format` and one `--out` and leaves the console empty,
/// so the app names a file of its own, reads it into the log and removes it. A file the
/// user named is theirs: it stays, and satz's own `wrote …` line names it in the log.
pub fn reports_dir() -> PathBuf {
    std::env::temp_dir().join("satz-studio-reports")
}

/// The file of [`reports_dir`] a run is about to write, from its arguments.
fn app_report(args: &[String]) -> Option<PathBuf> {
    let out = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))?;
    let path = PathBuf::from(out);
    path.starts_with(reports_dir()).then_some(path)
}

/// The report a run wrote into [`reports_dir`]: its text, and the file gone. A command
/// that exited zero and wrote nothing is a failure, not an empty report.
fn take_report(path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("reading the report at {}: {e}", path.display()))?;
    std::fs::remove_file(path)
        .map_err(|e| format!("removing the report at {}: {e}", path.display()))?;
    Ok(text)
}

/// The command line as the log header shows it.
pub fn command_line(dir: &Path, args: &[String]) -> String {
    let mut parts = vec![
        "satz".to_string(),
        "--config".to_string(),
        quote(&dir.display().to_string()),
    ];
    parts.extend(args.iter().map(|a| quote(a)));
    parts.join(" ")
}

/// Quote one shell word when it needs it.
pub fn quote(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:,@+".contains(c))
    {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

/// What follows a run: nothing, the estate read again because the command wrote it, or
/// the document an export wrote checked and opened.
#[derive(Debug, Clone, PartialEq, Eq)]
enum After {
    Nothing,
    Reload,
    Open(Exported),
}

/// The formats `satz questions` offers, read from the installed satz's help once per
/// session for the export cards. A help the app cannot read is a toast and the reason
/// the cards show, never an empty picker.
async fn read_export_formats(session: &Arc<EstateSession>, app: Store<AppStore>) {
    let formats = export::formats(&session.cli)
        .await
        .map_err(|e| e.to_string());
    if let Err(e) = &formats {
        toast(app, ToastKind::Error, format!("the export formats: {e}"));
    }
    app.estate().export_formats().set(Some(formats));
}

/// An export that exited zero: the file is checked — there and not empty — then
/// remembered for "Export again" and opened in the application the system gives its
/// type. A file that is not there is the run's failure; one that does not open is a
/// toast over a file that is written.
fn opened(app: Store<AppStore>, exported: Exported) -> Result<(), String> {
    export::written(&exported.path)?;
    let path = exported.path.clone();
    app.estate().last_export().set(Some(exported));
    match open::that(&path) {
        Ok(()) => toast(app, ToastKind::Info, format!("{} written", path.display())),
        Err(e) => toast(
            app,
            ToastKind::Error,
            format!("{} written, but it did not open: {e}", path.display()),
        ),
    }
    Ok(())
}

fn run_command(
    session: &Arc<EstateSession>,
    app: Store<AppStore>,
    args: Vec<String>,
    after: After,
) -> Option<CancellationToken> {
    if app.estate().running().cloned() {
        toast(app, ToastKind::Info, "a command is already running");
        return None;
    }
    let report = app_report(&args);
    if let Some(dir) = report.as_deref().and_then(Path::parent)
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        toast(app, ToastKind::Error, format!("{}: {e}", dir.display()));
        return None;
    }
    let token = CancellationToken::new();
    let (tx, mut lines) = tokio::sync::mpsc::channel::<CliLine>(256);
    let reread = Arc::clone(session);
    let cli = session.cli.clone();
    let argv = args.clone();
    let child = token.clone();
    let join = tokio::spawn(async move { cli.run(&argv, tx, child).await });

    let estate = app.estate();
    estate
        .last_command()
        .set(Some(command_line(&session.dir.dir, &args)));
    estate.command_log().clear();
    estate.outcome().set(None);
    estate.running().set(true);
    spawn(async move {
        while let Some(line) = lines.recv().await {
            let clean = match line {
                CliLine::Stdout(s) => CliLine::Stdout(strip_ansi(&s)),
                CliLine::Stderr(s) => CliLine::Stderr(strip_ansi(&s)),
            };
            estate.command_log().push(clean);
        }
        let mut outcome = match join.await {
            Ok(Ok(status)) => CommandOutcome {
                ok: status.success(),
                text: format!("exited with {status}"),
            },
            Ok(Err(e)) => CommandOutcome {
                ok: false,
                text: e.to_string(),
            },
            Err(e) => CommandOutcome {
                ok: false,
                text: format!("the command task failed: {e}"),
            },
        };
        if outcome.ok
            && let Some(path) = report
        {
            match take_report(&path) {
                Ok(text) => {
                    for line in text.lines() {
                        estate.command_log().push(CliLine::Stdout(line.to_string()));
                    }
                }
                Err(e) => outcome = CommandOutcome { ok: false, text: e },
            }
        }
        if outcome.ok
            && let After::Open(exported) = &after
            && let Err(e) = opened(app, exported.clone())
        {
            outcome = CommandOutcome { ok: false, text: e };
        }
        if !outcome.ok {
            toast(app, ToastKind::Error, outcome.text.clone());
        }
        estate.outcome().set(Some(outcome));
        estate.running().set(false);
        if after == After::Reload {
            reload(&reread, app).await;
        }
    });
    Some(token)
}

/// The directory satz asks git about before `merge-presets` edits the estate: the estate
/// file's own.
fn estate_file_dir(session: &EstateSession) -> PathBuf {
    session
        .main
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

/// `git <args…>` as the log header shows it.
fn git_line(args: &[String]) -> String {
    std::iter::once("git".to_string())
        .chain(args.iter().map(|a| quote(a)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The repository `satz merge-presets` needs: `git init -b main`, `git add -A` and one
/// commit naming the estate, run in the estate directory one after the other, streamed
/// into the log, stopping at the first one git refuses. It runs because the operator
/// pressed the Overview's button, and never otherwise.
///
/// Refused before git runs: a directory without a `.gitignore`, because `git add -A`
/// would commit whatever is there — the Terraform state and the provider schema with
/// the estate; an estate file outside the estate directory, which a repository there
/// would not hold; and, once running, a directory git already holds, where a second
/// repository would nest inside the first. The commit takes git's configured identity;
/// the app sets none, and without one git's own refusal is the log and the toast.
fn init_repository(
    session: &Arc<EstateSession>,
    app: Store<AppStore>,
) -> Option<CancellationToken> {
    if app.estate().running().cloned() {
        toast(app, ToastKind::Info, "a command is already running");
        return None;
    }
    let dir = session.dir.dir.clone();
    if !session.main.starts_with(&dir) {
        toast(
            app,
            ToastKind::Error,
            format!(
                "{} is outside the estate directory {}: a repository there would not hold it",
                session.main.display(),
                dir.display()
            ),
        );
        return None;
    }
    if !dir.join(".gitignore").is_file() {
        toast(
            app,
            ToastKind::Error,
            format!(
                "{}: no .gitignore — `git add -A` would commit the Terraform state and the provider schema with the estate. Write one first; satz init writes one.",
                dir.display()
            ),
        );
        return None;
    }
    let name = session
        .main
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let steps = git::init_steps(&name);
    let asked = estate_file_dir(session);

    let token = CancellationToken::new();
    let estate = app.estate();
    estate.last_command().set(Some(format!(
        "cd {} && {}",
        quote(&dir.display().to_string()),
        steps
            .iter()
            .map(|s| git_line(s))
            .collect::<Vec<_>>()
            .join(" && ")
    )));
    estate.command_log().clear();
    estate.outcome().set(None);
    estate.running().set(true);
    let cancel = token.clone();
    spawn(async move {
        let outcome = match WorkTree::read(&asked).await {
            WorkTree::Inside => CommandOutcome {
                ok: false,
                text: "git already holds this estate in a repository: nothing was run".to_string(),
            },
            WorkTree::NoGit(e) => CommandOutcome { ok: false, text: e },
            WorkTree::Outside(_) => run_steps(&dir, &steps, app, cancel).await,
        };
        if !outcome.ok {
            toast(app, ToastKind::Error, outcome.text.clone());
        }
        estate.outcome().set(Some(outcome));
        estate.work_tree().set(Some(WorkTree::read(&asked).await));
        estate.running().set(false);
    });
    Some(token)
}

/// Each git step in turn, its lines into the log; the first refusal ends the run with
/// git's last line on stderr, which is its reason.
async fn run_steps(
    dir: &Path,
    steps: &[Vec<String>],
    app: Store<AppStore>,
    cancel: CancellationToken,
) -> CommandOutcome {
    for step in steps {
        let line = git_line(step);
        let (tx, mut lines) = tokio::sync::mpsc::channel::<CliLine>(256);
        let join = {
            let (dir, step, cancel) = (dir.to_path_buf(), step.clone(), cancel.clone());
            tokio::spawn(async move { git::run(&dir, &step, tx, cancel).await })
        };
        let mut reason = None;
        while let Some(l) = lines.recv().await {
            if let CliLine::Stderr(s) = &l
                && !s.trim().is_empty()
            {
                reason = Some(s.clone());
            }
            app.estate().command_log().push(l);
        }
        let failed = match join.await {
            Ok(Ok(status)) if status.success() => continue,
            Ok(Ok(status)) => match reason {
                Some(reason) => format!("{line}: {reason}"),
                None => format!("{line} exited with {status}"),
            },
            Ok(Err(e)) => format!("{line}: {e}"),
            Err(e) => format!("{line}: the task failed: {e}"),
        };
        return CommandOutcome {
            ok: false,
            text: failed,
        };
    }
    CommandOutcome {
        ok: true,
        text: "the estate is in a repository of its own, with one commit".to_string(),
    }
}

/// `satz_merge_presets` on the session, into the log: the report typed and written as
/// sentences ([`MergeReport::lines`]), and the notices it opened queued — a merge that
/// brings a pack in, or a pack that gained one, opens notices of its own, and the same
/// window raises them. A refusal is satz's own text; a report the app cannot type is a
/// failure in the log's outcome and a toast, never the JSON it came as.
async fn merge_presets(session: &Arc<EstateSession>, app: Store<AppStore>) {
    const TOOL: &str = "satz_merge_presets";
    let estate = app.estate();
    estate.last_command().set(Some(TOOL.to_string()));
    estate.command_log().clear();
    estate.outcome().set(None);
    let outcome = match session.tool(TOOL, serde_json::Map::new()).await {
        Ok(o) if o.is_error => {
            for line in o.text.lines() {
                estate.command_log().push(CliLine::Stderr(line.to_string()));
            }
            toast(app, ToastKind::Error, format!("{TOOL}: {}", o.text));
            CommandOutcome {
                ok: false,
                text: format!("{TOOL} refused"),
            }
        }
        Ok(o) => match o.typed::<MergeReport>(TOOL) {
            Ok(report) => {
                for line in report.lines() {
                    estate.command_log().push(CliLine::Stdout(line));
                }
                queue_notices(app, &report.notices);
                let outcome = merge_outcome(&report);
                if !outcome.ok {
                    toast(app, ToastKind::Error, outcome.text.clone());
                }
                outcome
            }
            Err(e) => {
                toast(app, ToastKind::Error, e.to_string());
                CommandOutcome {
                    ok: false,
                    text: e.to_string(),
                }
            }
        },
        Err(e) => {
            toast(app, ToastKind::Error, format!("{TOOL}: {e}"));
            CommandOutcome {
                ok: false,
                text: e.to_string(),
            }
        }
    };
    estate.outcome().set(Some(outcome));
}

/// The outcome of a merge that returned a report: satz's own verdict. A report with
/// `attention` is what makes `satz merge-presets` exit non-zero, so it is a failed
/// outcome here, as a command that exits non-zero is in `run_command`.
fn merge_outcome(report: &MergeReport) -> CommandOutcome {
    if report.attention {
        CommandOutcome {
            ok: false,
            text: "satz_merge_presets: the merge needs attention".to_string(),
        }
    } else {
        CommandOutcome {
            ok: true,
            text: "satz_merge_presets returned".to_string(),
        }
    }
}

/// `satz review-pack` with the estate's config, through the CLI: the MCP tool is confined
/// to the estate's root, and a pack under review usually lives outside it. The review
/// replaces the last one; its findings join the drawer, which opens on a review that has
/// something to say above an info, and the verdict is a toast. A review that failed —
/// satz could not run, or answered in a shape the app does not read — keeps the pack's
/// path and satz's reason in the view.
async fn review_pack(
    session: &Arc<EstateSession>,
    app: Store<AppStore>,
    pack: PathBuf,
    against: bool,
) {
    let estate = against.then_some(session.main.as_path());
    match review::review(&session.cli, &pack, estate).await {
        Ok(reviewed) => {
            toast(app, ToastKind::Info, review_verdict(&reviewed.review));
            if reviewed
                .review
                .findings
                .iter()
                .any(|f| f.severity != FindingSeverity::Info)
            {
                app.drawer_open().set(true);
            }
            app.estate()
                .review()
                .set(Some(PackReviewState::Reviewed(reviewed)));
        }
        Err(e) => {
            let error = e.to_string();
            toast(app, ToastKind::Error, format!("review-pack: {error}"));
            app.estate()
                .review()
                .set(Some(PackReviewState::Failed { pack, error }));
        }
    }
}

/// The toast after a review: satz's verdict and what it counted.
pub fn review_verdict(review: &PackReview) -> String {
    let count = |n: usize, one: &str, many: &str| match n {
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    };
    let errors = review.count(FindingSeverity::Error);
    let warnings = review.count(FindingSeverity::Warning);
    if review.passed() {
        match warnings {
            0 => "the pack clears the bar".to_string(),
            n => format!(
                "the pack clears the bar · {}",
                count(n, "warning", "warnings")
            ),
        }
    } else {
        format!(
            "the pack does not clear the bar yet · {}",
            count(errors, "error", "errors")
        )
    }
}

/// Destination B: the reviewed bytes into the estate's `presets_dir` as
/// `<stem>.local.satz`, under the write lock and checked by `satz_transpile_check` with the
/// file in the library — removed again when the check refuses. Once placed, the placed
/// file is reviewed where it now stands, so the drawer names the file the estate reads.
async fn place_private(session: &Arc<EstateSession>, app: Store<AppStore>) {
    let Some(PackReviewState::Reviewed(reviewed)) = app.estate().review().cloned() else {
        toast(app, ToastKind::Error, "no reviewed pack to place");
        return;
    };
    let presets = session.dir.presets_dir();
    let checker = McpChecker {
        session: Arc::clone(session),
    };
    let placed = {
        let _lock = session.write_lock().await;
        review::place_private(&reviewed, &presets, &session.main, &checker).await
    };
    match placed {
        Ok(Placed::Written { path, .. }) => {
            toast(
                app,
                ToastKind::Info,
                format!("placed in the library as {}", path.display()),
            );
            review_pack(session, app, path, reviewed.against).await;
        }
        Ok(Placed::AlreadyThere(path)) => toast(
            app,
            ToastKind::Info,
            format!(
                "the library already holds this pack as {}: nothing written",
                path.display()
            ),
        ),
        Err(PlaceError::Rollback(diags)) => {
            let first = diags
                .iter()
                .find(|d| d.severity == Severity::Error)
                .or(diags.first())
                .map(|d| d.message.lines().next().unwrap_or_default().to_string())
                .unwrap_or_default();
            toast(
                app,
                ToastKind::Error,
                format!("not placed — the estate did not compile with it: {first}"),
            );
            app.estate().diagnostics().write().extend(diags);
            app.drawer_open().set(true);
        }
        Err(e) => toast(app, ToastKind::Error, e.to_string()),
    }
}

fn open_in_terminal(session: &Arc<EstateSession>, app: Store<AppStore>, args: &[String]) {
    let opened = session
        .external_command(args)
        .and_then(|script| EstateSession::open_in_terminal(&script).map(|()| script));
    match opened {
        Ok(script) => toast(
            app,
            ToastKind::Info,
            format!("Running in your terminal: {}", script.display()),
        ),
        Err(e) => toast(
            app,
            ToastKind::Error,
            format!("not opened in a terminal: {e}"),
        ),
    }
}

async fn reload(session: &Arc<EstateSession>, app: Store<AppStore>) {
    reload_with(session, app, Carried::default()).await;
}

/// `satz_transpile_check` on the estate as it stands: the findings of a compile that
/// passed, the diagnostics of one that refused, or the one error of a checker that
/// could not run.
async fn check(session: &Arc<EstateSession>) -> Vec<Diagnostic> {
    let checker = McpChecker {
        session: Arc::clone(session),
    };
    let base = session.main.parent().unwrap_or(Path::new("."));
    match checker.check(&session.main).await {
        Ok(summary) => summary
            .findings
            .iter()
            .map(|f| Diagnostic::from_finding(base, f, DiagSource::Check))
            .collect(),
        Err(CheckFailure::Refused(diags)) => diags,
        Err(CheckFailure::Failed(e)) => vec![Diagnostic::error(
            format!("satz_transpile_check: {e}"),
            DiagSource::Check,
        )],
    }
}

/// Questions and packs through the session, then the file, the params and the schema on
/// a blocking thread, then the model, then the compile's own check. Every failure is a
/// diagnostic and a toast; the model stays what it was. `carried` — what the write this
/// reload follows said — stays in the drawer, at its line in the file as it is: what the
/// check already said, so the reload does not run it again, and a tool's refusal.
async fn reload_with(session: &Arc<EstateSession>, app: Store<AppStore>, carried: Carried) {
    let estate = app.estate();
    estate.loading().set(true);
    estate.hcl().set(HclState::read(&session.dir.hcl_dir()));
    estate
        .work_tree()
        .set(Some(WorkTree::read(&estate_file_dir(session)).await));
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    let questions = match session
        .tool("satz_questions", serde_json::Map::new())
        .await
        .and_then(|o| o.typed::<QuestionsReport>("satz_questions"))
    {
        Ok(report) => Some(report),
        Err(e) => {
            diagnostics.push(Diagnostic::error(
                e.to_string(),
                DiagSource::Tool("satz_questions".to_string()),
            ));
            None
        }
    };
    estate.questions().set(questions.clone());

    let packs = match session
        .tool("satz_packs", serde_json::Map::new())
        .await
        .and_then(|o| o.typed::<PacksReport>("satz_packs"))
    {
        Ok(report) => Some(report),
        Err(e) => {
            diagnostics.push(Diagnostic::error(
                e.to_string(),
                DiagSource::Tool("satz_packs".to_string()),
            ));
            None
        }
    };

    let main = session.main.clone();
    let dir = session.dir.clone();
    // The error is boxed: a `Diagnostic` is a path, a message and a source, which is
    // large enough that clippy refuses it as the `Err` of a `Result` returned by value.
    let parsed = tokio::task::spawn_blocking(move || {
        let text = std::fs::read_to_string(&main).map_err(|e| {
            Box::new(Diagnostic::error(
                format!("{}: {e}", main.display()),
                DiagSource::Cst,
            ))
        })?;
        let cst = Cst::parse(&text)
            .map_err(|e| Box::new(Diagnostic::error(e.to_string(), DiagSource::Cst)))?;
        let env = dir
            .params(&main)
            .map_err(|e| Box::new(Diagnostic::from_pipeline_error(&dir.dir, &e)))?;
        let registry = ResourceRegistry::load_all(&dir.schema_dir());
        Ok::<_, Box<Diagnostic>>((cst, env, registry))
    })
    .await;

    let built = match parsed {
        Ok(Ok((cst, env, registry))) => {
            // What takes a notice off the window: the estate binds its param, whether
            // the operator pressed "I ran it" or the command the notice names bound it
            // itself. satz's own rule judges it, over the params of this reload.
            let held = estate.notices().cloned();
            if !held.is_empty() {
                estate.notices().set(
                    held.into_iter()
                        .filter(|n| !EstateDir::acknowledged(&env, &n.param))
                        .collect(),
                );
            }
            let schema_dir = session.dir.schema_dir();
            let registry = match registry {
                Ok(r) => Some(r),
                Err(SchemaError::Missing(_)) => None,
                Err(e) => {
                    diagnostics.push(Diagnostic::error(e.to_string(), DiagSource::Compile));
                    None
                }
            };
            questions.as_ref().zip(packs.as_ref()).map(|(q, p)| {
                EstateModel::build(
                    &session.main,
                    &cst,
                    registry.as_ref().ok_or(schema_dir.as_path()),
                    &env,
                    q,
                    p,
                    diagnostics.clone(),
                )
                .map(|model| (model, cst))
                .map_err(|e| Box::new(Diagnostic::error(e.to_string(), DiagSource::Compile)))
            })
        }
        Ok(Err(d)) => {
            diagnostics.push(*d);
            None
        }
        Err(e) => {
            diagnostics.push(Diagnostic::error(
                format!("the reload task failed: {e}"),
                DiagSource::Compile,
            ));
            None
        }
    };

    let mut built_ok = false;
    match built {
        Some(Ok((model, cst))) => {
            built_ok = true;
            diagnostics = model.diagnostics.clone();
            estate.model().set(Some(Arc::new(model)));
            estate.cst().set(Some(Arc::new(cst)));
        }
        Some(Err(d)) => diagnostics.push(*d),
        None => {}
    }
    // What the compile finds after the front end — a role the IaC service account
    // lacks, a required argument the provider wants, raw HCL nobody has reviewed — is
    // data the estate carries and the app has no other way to learn. It is read here,
    // once per reload, and only when the front end got as far as a model: a file the
    // front end refused has already said why, and the check would say it twice.
    if built_ok && carried.checked.is_empty() {
        diagnostics.extend(check(session).await);
    }
    diagnostics.extend(carried.checked);
    diagnostics.extend(carried.refused);
    if let Some(first) = diagnostics
        .iter()
        .find(|d| d.severity == satz_studio_core::diag::Severity::Error)
    {
        toast(
            app,
            ToastKind::Error,
            first.message.lines().next().unwrap_or_default().to_string(),
        );
    }
    estate.diagnostics().set(diagnostics);
    estate.loading().set(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(param: &str) -> NoticeRow {
        NoticeRow {
            param: param.to_string(),
            pack: "presets/cis/block-project-ssh-keys.satz".to_string(),
            text: "Import what is live first.".to_string(),
            run: "satz adopt <estate> --execute --import".to_string(),
            severity: satz_studio_core::satz::reports::FindingSeverity::Error,
            acknowledged: false,
        }
    }

    #[test]
    fn a_notice_is_held_once_and_a_later_call_neither_drops_nor_doubles_it() {
        let opened = vec![notice("cis_baseline_adopted")];
        let held = queued(&[], &opened);
        assert_eq!(held.len(), 1);
        // satz returns a notice once, in the call that opened it: a later call that
        // returns none leaves the window holding it
        assert_eq!(queued(&held, &[]), held);
        // and a call that returns it again adds nothing
        assert_eq!(queued(&held, &opened), held);
    }

    #[test]
    fn a_notice_the_estate_has_already_acknowledged_is_never_held() {
        let mut done = notice("cis_baseline_adopted");
        done.acknowledged = true;
        assert!(queued(&[], &[done]).is_empty());
    }

    #[test]
    fn a_refused_second_command_leaves_the_first_one_cancellable() {
        let mut running = RunningCommand::default();
        let first = CancellationToken::new();
        running.started(Some(first.clone()));
        // `run_command` refuses while a command runs and hands back no token
        running.started(None);
        running.cancel();
        assert!(
            first.is_cancelled(),
            "Cancel still reaches the command that is running"
        );
    }

    #[test]
    fn the_next_command_is_the_one_cancel_reaches() {
        let mut running = RunningCommand::default();
        let first = CancellationToken::new();
        let second = CancellationToken::new();
        running.started(Some(first.clone()));
        running.started(Some(second.clone()));
        running.cancel();
        assert!(second.is_cancelled());
        assert!(!first.is_cancelled());
    }

    fn change(action: &str, switched: &[&str]) -> PackChange {
        PackChange {
            estate: "C0example.satz".to_string(),
            action: action.to_string(),
            switched: switched.iter().map(|s| s.to_string()).collect(),
            bound: Vec::new(),
            lines: Vec::new(),
            left: Vec::new(),
            opened: Vec::new(),
            notices: Vec::new(),
        }
    }

    #[test]
    fn a_switch_says_what_it_switched_and_what_it_opened() {
        assert_eq!(
            switched(&change("add", &["presets/organization-budget.satz"])),
            "presets/organization-budget.satz on"
        );
        let mut two = change(
            "remove",
            &[
                "presets/scc/scc-notifications.satz",
                "presets/scc/scc-findings-mail.satz",
            ],
        );
        assert_eq!(switched(&two), "2 packs off");
        two.action = "add".to_string();
        two.opened = vec!["scc_notification_topic".to_string()];
        two.notices = vec![notice("cis_baseline_adopted")];
        assert_eq!(
            switched(&two),
            "2 packs on · 1 question opened · 1 notice opened"
        );
        let mut nothing = change("add", &[]);
        nothing.left = vec!["already on — nothing to write".to_string()];
        assert_eq!(switched(&nothing), "already on — nothing to write");
    }

    /// satz's verdict decides the chip: `attention` is what makes `satz merge-presets`
    /// exit non-zero, so it is a failed outcome; without it the merge is ok.
    #[test]
    fn a_merge_that_needs_attention_is_a_failed_outcome() {
        let mut report: MergeReport = serde_json::from_value(serde_json::json!({
            "report_only": false,
            "events": [],
            "counts": {
                "installed": 0, "current": 1, "artifacts_updated": 0, "doc_only": 0,
                "unused_overwritten": 0, "adopted_in_place": 0, "forked_and_repointed": 0,
                "fork_diffs_refreshed": 0, "deferred": 0, "refused": 0, "skipped_edited": 0
            },
            "attention": true
        }))
        .unwrap();
        let failed = merge_outcome(&report);
        assert!(!failed.ok);
        assert!(failed.text.contains("attention"), "{}", failed.text);
        report.attention = false;
        assert!(merge_outcome(&report).ok);
    }

    /// The toast says satz's verdict and counts what decides it: the errors of a pack that
    /// does not clear the bar, the warnings of one that does.
    #[test]
    fn a_review_toast_says_the_verdict_and_what_it_counted() {
        let broken: PackReview = serde_json::from_str(include_str!(
            "../../../satz-studio-core/tests/fixtures/review/team-access.json"
        ))
        .unwrap();
        assert_eq!(
            review_verdict(&broken),
            "the pack does not clear the bar yet · 3 errors"
        );
        let clean: PackReview = serde_json::from_str(include_str!(
            "../../../satz-studio-core/tests/fixtures/review/organization-budget.json"
        ))
        .unwrap();
        assert_eq!(review_verdict(&clean), "the pack clears the bar");
    }

    #[test]
    fn plain_words_stay_and_spaces_are_quoted() {
        assert_eq!(quote("C0example.satz"), "C0example.satz");
        assert_eq!(quote("--format=json"), "--format=json");
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), "'it'\\''s'");
        assert_eq!(quote(""), "''");
    }

    #[test]
    fn the_header_names_the_config_and_every_argument() {
        let line = command_line(
            Path::new("/estates/acme"),
            &[
                "transpile".into(),
                "C0example.satz".into(),
                "--check".into(),
            ],
        );
        assert_eq!(
            line,
            "satz --config /estates/acme transpile C0example.satz --check"
        );
    }
}
