//! The per-estate coroutine: one per open session, owning its `Arc<EstateSession>`.
//! It reloads the model, runs CLI commands with their output streamed into the store,
//! calls tools, hands `apply` and `bootstrap` to the OS terminal, and is the one place
//! the estate is written from: an answer through satz's own writer, a value through
//! the app's, the map line by the app itself — each under the session's write lock,
//! each verified by `satz transpile --check`, each followed by a reload. A running
//! command streams from a tokio task into a local task, so the loop stays free to take
//! `CancelCommand`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use dioxus::prelude::*;
use futures_util::StreamExt;
use satz_studio_core::cst::{Cst, Span, UseState, scan_uses};
use satz_studio_core::diag::{DiagSource, Diagnostic, Severity};
use satz_studio_core::edit::snapshot::Snapshot;
use satz_studio_core::edit::{CommitError, Edit, EditSession, McpChecker, Rollback};
use satz_studio_core::model::{EstateModel, MAP_PATH};
use satz_studio_core::satz::reports::{InterviewArgs, InterviewReport, QuestionsReport};
use satz_studio_core::satz::{CliLine, EstateSession};
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
    /// `satz --config <dir> <args…>`, streamed into the log
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
    /// the app's own writer: the edit applied in memory, checked as a temp file
    /// beside the real one, renamed over it
    CommitEdit(Edit),
    /// uncomment `// use "presets/estate-map.satz"`, the one pack line no question
    /// gates and so no satz writer activates
    EnableMap,
    /// `satz_merge_presets`: the line for a pack the library gained
    MergePresets,
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
            EstateAction::Close => close_estate(app),
        }
    }
}

/// A delegated write: `satz_interview` writes the real file, so the bytes are recorded
/// first and the check runs on the real path afterwards; a refusal restores them. A
/// refused tool call wrote nothing and is satz's own sentence in a toast.
async fn interview(session: &Arc<EstateSession>, app: Store<AppStore>, args: InterviewArgs) {
    let carried = {
        let _lock = session.write_lock().await;
        let snapshot = match Snapshot::take(&session.main) {
            Ok(s) => s,
            Err(e) => {
                toast(app, ToastKind::Error, e.to_string());
                return;
            }
        };
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
        let outcome = match session.tool("satz_interview", args).await {
            Ok(o) => o,
            Err(e) => {
                toast(app, ToastKind::Error, format!("satz_interview: {e}"));
                return;
            }
        };
        if outcome.is_error {
            toast(app, ToastKind::Error, outcome.text.clone());
            return;
        }
        let report = outcome.typed::<InterviewReport>("satz_interview");
        let checker = McpChecker {
            session: Arc::clone(session),
        };
        match snapshot.verify(&checker).await {
            Ok(_) => match report {
                Ok(report) => {
                    let written = report.written;
                    app.estate().interview().set(Some(report));
                    toast(
                        app,
                        ToastKind::Info,
                        match written {
                            1 => "1 answer written".to_string(),
                            n => format!("{n} answers written"),
                        },
                    );
                    Vec::new()
                }
                Err(e) => {
                    toast(app, ToastKind::Error, format!("satz_interview: {e}"));
                    Vec::new()
                }
            },
            Err(e) => rolled_back(app, e),
        }
    };
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
                Vec::new()
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
            Ok(_) => {
                toast(
                    app,
                    ToastKind::Info,
                    "the map is in: its questions are open",
                );
                Vec::new()
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
        let outcome = match join.await {
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
        if !outcome.ok {
            toast(app, ToastKind::Error, outcome.text.clone());
        }
        estate.outcome().set(Some(outcome));
        estate.running().set(false);
    });
    Some(token)
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

/// Questions through the session, then the file, the params and the schema on a
/// blocking thread, then the model. Every failure is a diagnostic and a toast; the
/// model stays what it was. `carried` — a refused write's diagnostics — stays in the
/// drawer after the reload, at its line in the file as it is.
async fn reload_with(session: &Arc<EstateSession>, app: Store<AppStore>, carried: Vec<Diagnostic>) {
    let estate = app.estate();
    estate.loading().set(true);
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
    let parsed = tokio::task::spawn_blocking(move || {
        let text = std::fs::read_to_string(&main)
            .map_err(|e| Diagnostic::error(format!("{}: {e}", main.display()), DiagSource::Cst))?;
        let cst =
            Cst::parse(&text).map_err(|e| Diagnostic::error(e.to_string(), DiagSource::Cst))?;
        let env = dir
            .params(&main)
            .map_err(|e| Diagnostic::from_pipeline_error(&dir.dir, &e))?;
        let registry = ResourceRegistry::load_all(&dir.schema_dir());
        Ok::<_, Diagnostic>((cst, env, registry))
    })
    .await;

    let built = match parsed {
        Ok(Ok((cst, env, registry))) => {
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
                    diagnostics.clone(),
                )
                .map(|model| (model, cst))
                .map_err(|e| Diagnostic::error(e.to_string(), DiagSource::Compile))
            })
        }
        Ok(Err(d)) => {
            diagnostics.push(d);
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

    match built {
        Some(Ok((model, cst))) => {
            diagnostics = model.diagnostics.clone();
            estate.model().set(Some(Arc::new(model)));
            estate.cst().set(Some(Arc::new(cst)));
        }
        Some(Err(d)) => diagnostics.push(d),
        None => {}
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
