//! The per-estate coroutine: one per open session, owning its `Arc<EstateSession>`.
//! It reloads the model, runs CLI commands with their output streamed into the store,
//! calls tools, hands `apply` and `bootstrap` to the OS terminal, and is the one place
//! the estate is written from: an answer through satz's own writer, a value through
//! the app's, the map line by the app itself — each under the session's write lock,
//! each verified by `satz transpile --check`, each followed by a reload. A running
//! command streams from a tokio task into a local task, so the loop stays free to take
//! `CancelCommand`; the three git commands that put the estate in a repository run the
//! same way, when the operator asks for them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dioxus::prelude::*;
use futures_util::StreamExt;
use satz_studio_core::cst::{Cst, Span, UseState, scan_uses};
use satz_studio_core::diag::{DiagSource, Diagnostic, Severity};
use satz_studio_core::edit::snapshot::Snapshot;
use satz_studio_core::edit::{
    CheckFailure, Checker, CommitError, Committed, Edit, EditSession, McpChecker, Rollback,
};
use satz_studio_core::estate::HclState;
use satz_studio_core::git::{self, WorkTree};
use satz_studio_core::model::{EstateModel, MAP_PATH, PackDecls};
use satz_studio_core::satz::reports::{
    InterviewArgs, InterviewReport, PrerequisitesResult, QuestionsReport,
};
use satz_studio_core::satz::{CliLine, EstateSession, ToolOutcome};
use satz_studio_core::schema::{ResourceRegistry, SchemaError};
use tokio_util::sync::CancellationToken;

use super::app_actions::close_estate;
use super::{
    AppStore, AppStoreStoreExt, CommandOutcome, EstateStore, EstateStoreStoreExt, ToastKind,
    strip_ansi, toast,
};

pub enum EstateAction {
    /// questions, parse, params, schema, model — after every write and on demand
    Reload,
    /// `satz --config <dir> <args…>`, streamed into the log; a reporting command's
    /// file goes into the log after it, whole
    RunCommand(Vec<String>),
    CancelCommand,
    /// one MCP tool on this estate's session, its result into the log
    RunTool {
        name: String,
        args: serde_json::Map<String, serde_json::Value>,
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
    /// uncomment `// use "presets/estate-map.satz"`, the one pack line no question
    /// gates and so no satz writer activates
    EnableMap,
    /// `satz_merge_presets`: the line for a pack the library gained
    MergePresets,
    /// `git init -b main`, `git add -A` and one commit in the estate directory, streamed
    /// into the log: the repository `satz merge-presets` needs for its undo
    InitRepository,
    Close,
}

pub async fn estate_coroutine(
    mut rx: UnboundedReceiver<EstateAction>,
    session: Arc<EstateSession>,
    app: Store<AppStore>,
) {
    app.estate().set(EstateStore::default());
    reload(&session, app).await;
    let mut cancel: Option<CancellationToken> = None;
    while let Some(action) = rx.next().await {
        match action {
            EstateAction::Reload => reload(&session, app).await,
            EstateAction::RunCommand(args) => cancel = run_command(&session, app, args),
            EstateAction::CancelCommand => {
                if let Some(token) = cancel.take() {
                    token.cancel();
                }
            }
            EstateAction::RunTool { name, args } => run_tool(&session, app, name, args).await,
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
            EstateAction::EnableMap => enable_map(&session, app).await,
            EstateAction::MergePresets => {
                {
                    let _lock = session.write_lock().await;
                    run_tool(
                        &session,
                        app,
                        "satz_merge_presets".to_string(),
                        serde_json::Map::new(),
                    )
                    .await;
                }
                reload(&session, app).await;
            }
            EstateAction::InitRepository => {
                if let Some(token) = init_repository(&session, app) {
                    cancel = Some(token);
                }
            }
            EstateAction::Close => close_estate(app),
        }
    }
}

/// A delegated write: satz's own writer works on the real file, so the bytes are
/// recorded first and the check runs on the real path afterwards; a refusal restores
/// them. A refused tool call wrote nothing and is satz's own sentence in a toast.
/// `landed` reads the outcome of a call that landed and says what the toast says —
/// `Err` for an outcome the app could not type, which is a toast in the error colour
/// over a write that is already on disk.
///
/// The returned diagnostics are what the reload carries: the findings of a check that
/// passed, or the refusal's own.
async fn delegated_write<F>(
    session: &Arc<EstateSession>,
    app: Store<AppStore>,
    name: &str,
    args: serde_json::Map<String, serde_json::Value>,
    landed: F,
) -> Vec<Diagnostic>
where
    F: FnOnce(&ToolOutcome) -> Result<String, String>,
{
    let _lock = session.write_lock().await;
    let snapshot = match Snapshot::take(&session.main) {
        Ok(s) => s,
        Err(e) => {
            toast(app, ToastKind::Error, e.to_string());
            return Vec::new();
        }
    };
    let outcome = match session.tool(name, args).await {
        Ok(o) => o,
        Err(e) => {
            toast(app, ToastKind::Error, format!("{name}: {e}"));
            return Vec::new();
        }
    };
    if outcome.is_error {
        toast(app, ToastKind::Error, outcome.text.clone());
        return Vec::new();
    }
    let checker = McpChecker {
        session: Arc::clone(session),
    };
    match snapshot.verify(&checker).await {
        Ok(committed) => {
            match landed(&outcome) {
                Ok(text) => toast(app, ToastKind::Info, text),
                Err(e) => toast(app, ToastKind::Error, e),
            }
            carried_findings(&committed)
        }
        Err(e) => rolled_back(app, e),
    }
}

/// One answer, or every default: `satz_interview` on the real file.
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
        app.estate().interview().set(Some(report));
        Ok(match written {
            1 => "1 answer written".to_string(),
            n => format!("{n} answers written"),
        })
    })
    .await;
    reload_with(session, app, carried).await;
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
    reload_with(session, app, carried).await;
}

/// The map line is the one pack line no question gates, so no satz writer activates
/// it: the app splices the exact line `pack_line` writes without its `// `, under the
/// delegated-write discipline — the bytes recorded, the real path checked, a refusal
/// restored. Nothing else is ever uncommented by the app.
async fn enable_map(session: &Arc<EstateSession>, app: Store<AppStore>) {
    let carried = {
        let _lock = session.write_lock().await;
        let snapshot = match Snapshot::take(&session.main) {
            Ok(s) => s,
            Err(e) => {
                toast(app, ToastKind::Error, e.to_string());
                return;
            }
        };
        let text = match std::str::from_utf8(snapshot.bytes()) {
            Ok(t) => t.to_string(),
            Err(e) => {
                toast(
                    app,
                    ToastKind::Error,
                    format!("{}: not UTF-8: {e}", session.main.display()),
                );
                return;
            }
        };
        let cst = match Cst::parse(&text) {
            Ok(c) => c,
            Err(e) => {
                toast(app, ToastKind::Error, e.to_string());
                return;
            }
        };
        let Some(line) = scan_uses(&cst).into_iter().find(|u| {
            u.path == MAP_PATH
                && u.gate.is_none()
                && u.as_key.is_none()
                && u.state == UseState::Commented
        }) else {
            toast(
                app,
                ToastKind::Error,
                format!("no commented `use \"{MAP_PATH}\"` line in this estate"),
            );
            return;
        };
        let new_text = match uncomment_line(&text, line.span) {
            Ok(t) => t,
            Err(e) => {
                toast(app, ToastKind::Error, e);
                return;
            }
        };
        if let Err(e) = std::fs::write(&session.main, new_text) {
            toast(
                app,
                ToastKind::Error,
                format!("{}: {e}", session.main.display()),
            );
            return;
        }
        let checker = McpChecker {
            session: Arc::clone(session),
        };
        match snapshot.verify(&checker).await {
            Ok(committed) => {
                toast(
                    app,
                    ToastKind::Info,
                    "the map is in: its questions are open",
                );
                carried_findings(&committed)
            }
            Err(e) => rolled_back(app, e),
        }
    };
    reload_with(session, app, carried).await;
}

/// The line at `span` with its `// ` removed and its indentation kept.
pub fn uncomment_line(text: &str, span: Span) -> Result<String, String> {
    let line = &text[span.start..span.end];
    let body = line.trim_start();
    let indent = &line[..line.len() - body.len()];
    let Some(rest) = body.strip_prefix("// ") else {
        return Err(format!("not a commented line: `{line}`"));
    };
    Ok(format!(
        "{}{indent}{rest}{}",
        &text[..span.start],
        &text[span.end..]
    ))
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

fn run_command(
    session: &Arc<EstateSession>,
    app: Store<AppStore>,
    args: Vec<String>,
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
        if !outcome.ok {
            toast(app, ToastKind::Error, outcome.text.clone());
        }
        estate.outcome().set(Some(outcome));
        estate.running().set(false);
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

async fn run_tool(
    session: &Arc<EstateSession>,
    app: Store<AppStore>,
    name: String,
    args: serde_json::Map<String, serde_json::Value>,
) {
    let estate = app.estate();
    let shown = serde_json::to_string(&args).unwrap_or_default();
    estate.last_command().set(Some(format!("{name} {shown}")));
    estate.command_log().clear();
    estate.outcome().set(None);
    match session.tool(&name, args).await {
        Ok(outcome) => {
            for line in outcome.text.lines() {
                estate.command_log().push(CliLine::Stdout(line.to_string()));
            }
            if let Some(structured) = &outcome.structured {
                let pretty = serde_json::to_string_pretty(structured).unwrap_or_default();
                for line in pretty.lines() {
                    estate.command_log().push(CliLine::Stdout(line.to_string()));
                }
            }
            let text = if outcome.is_error {
                format!("{name} refused")
            } else {
                format!("{name} returned")
            };
            if outcome.is_error {
                toast(app, ToastKind::Error, format!("{name}: {}", outcome.text));
            }
            estate.outcome().set(Some(CommandOutcome {
                ok: !outcome.is_error,
                text,
            }));
        }
        Err(e) => {
            toast(app, ToastKind::Error, format!("{name}: {e}"));
            estate.outcome().set(Some(CommandOutcome {
                ok: false,
                text: e.to_string(),
            }));
        }
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
    reload_with(session, app, Vec::new()).await;
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

/// Questions through the session, then the file, the params and the schema on a
/// blocking thread, then the model, then the compile's own check. Every failure is a
/// diagnostic and a toast; the model stays what it was. `carried` — the diagnostics of
/// the write this reload follows — stays in the drawer, at its line in the file as it
/// is, and is what the check already said, so the reload does not run it again.
async fn reload_with(session: &Arc<EstateSession>, app: Store<AppStore>, carried: Vec<Diagnostic>) {
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
        // what the packs declare between the choices is read here, off the async task,
        // with the loader the params fold above used
        let decls = PackDecls::read(&main, &cst, &dir.loader(&main));
        Ok::<_, Box<Diagnostic>>((cst, env, registry, decls))
    })
    .await;

    let built = match parsed {
        Ok(Ok((cst, env, registry, decls))) => {
            let schema_dir = session.dir.schema_dir();
            let registry = match registry {
                Ok(r) => Some(r),
                Err(SchemaError::Missing(_)) => None,
                Err(e) => {
                    diagnostics.push(Diagnostic::error(e.to_string(), DiagSource::Compile));
                    None
                }
            };
            questions.as_ref().map(|q| {
                EstateModel::build(
                    &session.main,
                    &cst,
                    registry.as_ref().ok_or(schema_dir.as_path()),
                    &env,
                    q,
                    &decls,
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
    if built_ok && carried.is_empty() {
        diagnostics.extend(check(session).await);
    }
    diagnostics.extend(carried);
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

    #[test]
    fn the_map_line_loses_its_marker_and_keeps_its_indentation_and_neighbours() {
        let text = "estate e\n\n// the map\n  // use \"presets/estate-map.satz\"\nuse \"x.satz\"\n";
        let cst = Cst::parse(text).unwrap();
        let line = scan_uses(&cst)
            .into_iter()
            .find(|u| u.path == MAP_PATH)
            .unwrap();
        assert_eq!(line.state, UseState::Commented);
        assert_eq!(
            uncomment_line(text, line.span).unwrap(),
            "estate e\n\n// the map\n  use \"presets/estate-map.satz\"\nuse \"x.satz\"\n"
        );
    }

    #[test]
    fn a_line_without_the_marker_is_refused() {
        let text = "use \"presets/estate-map.satz\"\n";
        let span = Span {
            start: 0,
            end: text.len() - 1,
        };
        assert!(
            uncomment_line(text, span)
                .unwrap_err()
                .starts_with("not a commented line")
        );
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
