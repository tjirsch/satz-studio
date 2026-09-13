//! The per-estate coroutine: one per open session, owning its `Arc<EstateSession>`.
//! It reloads the model, runs CLI commands with their output streamed into the store,
//! calls tools, and hands `apply` and `bootstrap` to the OS terminal. A running command
//! streams from a tokio task into a local task, so the loop stays free to take
//! `CancelCommand`.

use std::path::Path;
use std::sync::Arc;

use dioxus::prelude::*;
use futures_util::StreamExt;
use satz_studio_core::cst::Cst;
use satz_studio_core::diag::{DiagSource, Diagnostic};
use satz_studio_core::model::EstateModel;
use satz_studio_core::satz::reports::QuestionsReport;
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
            EstateAction::Close => close_estate(app),
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

/// Questions through the session, then the file, the params and the schema on a
/// blocking thread, then the model. Every failure is a diagnostic and a toast; the
/// model stays what it was.
async fn reload(session: &Arc<EstateSession>, app: Store<AppStore>) {
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
                    registry.as_ref(),
                    &env,
                    q,
                    diagnostics.clone(),
                )
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
        Some(Ok(model)) => {
            diagnostics = model.diagnostics.clone();
            estate.model().set(Some(Arc::new(model)));
        }
        Some(Err(d)) => diagnostics.push(d),
        None => {}
    }
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
