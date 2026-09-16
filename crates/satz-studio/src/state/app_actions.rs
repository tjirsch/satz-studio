//! The app coroutine: satz, the estate folder, sessions, settings and the credential.
//! It runs for the life of the window; the views send [`AppAction`]s and read the
//! store.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dioxus::prelude::*;
use futures_util::StreamExt;
use satz_studio_core::estate::EstateDir;
use satz_studio_core::llm::Credential;
use satz_studio_core::satz::{CliLine, EstateSession, InitOptions, SatzBinary, SatzCli, SatzError};
use satz_studio_core::settings::Settings;
use tokio_util::sync::CancellationToken;

use super::{
    AppStore, AppStoreStoreExt, CommandOutcome, CreateStoreStoreExt, CredentialStatus, EstateFile,
    EstateStore, EstateSummary, OpenEstate, SatzStatus, ToastKind, UpdateStoreStoreExt, quote,
    strip_ansi, toast,
};

pub enum AppAction {
    /// find satz again (the Settings path changed)
    LocateSatz,
    /// walk a folder for `config.toml` files and remember it as `last_root`
    Discover(PathBuf),
    /// open a session on one estate file of one config
    OpenEstate {
        config: PathBuf,
        estate: PathBuf,
    },
    CloseEstate,
    /// `satz init` in `dir` with no `--config`, streamed into the create log; on a run
    /// that made one estate, that estate is opened
    CreateEstate {
        dir: PathBuf,
        options: InitOptions,
    },
    CancelCreate,
    /// `satz self-update` on the satz that is installed, streamed into the update log,
    /// followed by locating satz again so the new version is the one in use. satz owns
    /// its own updater; the app only runs it. `check_only` passes `--check-only`, which
    /// reports without installing.
    UpdateSatz {
        check_only: bool,
    },
    CancelUpdate,
    /// write the settings file, then locate satz again
    SaveSettings(Settings),
    ResolveCredential,
    /// put a key in the OS keychain, then resolve again
    StoreKey(String),
}

pub async fn app_coroutine(mut rx: UnboundedReceiver<AppAction>, app: Store<AppStore>) {
    locate(app).await;
    if let Some(root) = app.root().cloned() {
        discover(app, root).await;
    }
    // `init` is a live command against Google and can take a while; it streams from a
    // task so this loop stays free to take `CancelCreate`, as a command does.
    let mut creating: Option<CancellationToken> = None;
    let mut updating: Option<CancellationToken> = None;
    while let Some(action) = rx.next().await {
        match action {
            AppAction::LocateSatz => locate(app).await,
            AppAction::Discover(root) => discover(app, root).await,
            AppAction::OpenEstate { config, estate } => open_estate(app, config, estate).await,
            AppAction::CloseEstate => close_estate(app),
            AppAction::CreateEstate { dir, options } => {
                creating = create_estate(app, dir, options);
            }
            AppAction::CancelCreate => {
                if let Some(token) = creating.take() {
                    token.cancel();
                }
            }
            AppAction::UpdateSatz { check_only } => {
                updating = update_satz(app, check_only);
            }
            AppAction::CancelUpdate => {
                if let Some(token) = updating.take() {
                    token.cancel();
                }
            }
            // the toast said what went wrong; the view keeps the draft either way
            AppAction::SaveSettings(settings) => {
                let _ = save_settings(app, settings).await;
            }
            AppAction::ResolveCredential => resolve_credential(app).await,
            AppAction::StoreKey(key) => store_key(app, key).await,
        }
    }
}

/// `satz self-update`, streamed into the update log, then satz is located again.
///
/// satz owns its own updater — it checks GitHub, verifies the sha256 sidecar and runs the
/// installer — so the app runs that and shows what it said rather than fetching anything
/// itself. Two things it must get right. `--no-open-readme`, because without it a
/// successful update opens the documentation site in a browser behind the operator, which
/// is satz's right behaviour on a terminal and the wrong one under a window. And the
/// binary it runs is [`SatzStatus::updatable`], which answers for a satz that is TOO OLD
/// as well as a current one: a too-old satz updating itself is the entire point of the
/// offer, and the path is the one the refusal carried.
///
/// The operator's `self_update_frequency` is theirs; this writes no satz configuration.
fn update_satz(app: Store<AppStore>, check_only: bool) -> Option<CancellationToken> {
    let Some(path) = app.satz().read().updatable().map(Path::to_path_buf) else {
        toast(
            app,
            ToastKind::Error,
            "there is no satz to update — see the banner",
        );
        return None;
    };
    let mut args = vec!["self-update".to_string(), "--no-open-readme".to_string()];
    if check_only {
        args.push("--check-only".to_string());
    }
    // `self-update` reads no estate, so the working directory only has to exist; the
    // temporary directory always does, and nothing is written into it.
    let dir = std::env::temp_dir();
    let token = CancellationToken::new();
    let (tx, mut lines) = tokio::sync::mpsc::channel::<CliLine>(256);
    let child = token.clone();
    let argv = args.clone();
    let satz = path.clone();
    let join = tokio::spawn(async move { SatzCli::run_in(&satz, &dir, &argv, tx, child).await });

    let update = app.update();
    update
        .command()
        .set(Some(format!("{} {}", path.display(), args.join(" "))));
    update.log().clear();
    update.outcome().set(None);
    update.running().set(true);
    spawn(async move {
        let mut last_stderr = None;
        while let Some(line) = lines.recv().await {
            let clean = match line {
                CliLine::Stdout(s) => CliLine::Stdout(strip_ansi(&s)),
                CliLine::Stderr(s) => {
                    let s = strip_ansi(&s);
                    if !s.trim().is_empty() {
                        last_stderr = Some(s.clone());
                    }
                    CliLine::Stderr(s)
                }
            };
            update.log().push(clean);
        }
        let ended = join.await;
        let cancelled = matches!(ended, Ok(Err(SatzError::Cancelled)));
        let outcome = match ended {
            Ok(Ok(status)) if !status.success() => CommandOutcome {
                ok: false,
                text: last_stderr.unwrap_or_else(|| format!("exited with {status}")),
            },
            Ok(Ok(_)) if check_only => CommandOutcome {
                ok: true,
                text: "checked — the log has what satz found".to_string(),
            },
            Ok(Ok(_)) => CommandOutcome {
                ok: true,
                text: "satz updated".to_string(),
            },
            Ok(Err(SatzError::Cancelled)) => CommandOutcome {
                ok: false,
                text: "cancelled — satz is as it was".to_string(),
            },
            Ok(Err(e)) => CommandOutcome {
                ok: false,
                text: e.to_string(),
            },
            Err(e) => CommandOutcome {
                ok: false,
                text: format!("the update task failed: {e}"),
            },
        };
        if !outcome.ok && !cancelled {
            toast(app, ToastKind::Error, outcome.text.clone());
        }
        let installed = outcome.ok && !check_only;
        app.update().outcome().set(Some(outcome));
        app.update().running().set(false);
        // The version on disk changed, so the one the app holds is stale.
        if installed {
            locate(app).await;
        }
    });
    Some(token)
}

/// Drop the session: the estate host unmounts and its coroutine with it.
pub fn close_estate(app: Store<AppStore>) {
    app.open().set(None);
    app.estate().set(EstateStore::default());
}

async fn locate(app: Store<AppStore>) {
    let override_path = app.settings().read().satz_binary.clone();
    let status = match SatzBinary::locate(override_path.as_deref()).await {
        Ok(bin) => SatzStatus::Located(bin),
        Err(SatzError::TooOld {
            path,
            found,
            required,
        }) => SatzStatus::TooOld {
            path,
            found: found.to_string(),
            required: required.to_string(),
        },
        Err(e) => SatzStatus::Missing(e.to_string()),
    };
    tracing::info!(?status, "satz located");
    app.satz().set(status);
}

async fn discover(app: Store<AppStore>, root: PathBuf) {
    app.discovering().set(true);
    app.root().set(Some(root.clone()));
    let walked = root.clone();
    let found = tokio::task::spawn_blocking(move || walk(&walked)).await;
    match found {
        Ok(estates) => {
            if estates.is_empty() {
                toast(
                    app,
                    ToastKind::Info,
                    format!("No config.toml under {}", root.display()),
                );
            }
            app.estates().set(estates);
        }
        Err(e) => toast(
            app,
            ToastKind::Error,
            format!("walking {}: {e}", root.display()),
        ),
    }
    app.discovering().set(false);
    let mut settings = app.settings().read().clone();
    if settings.last_root.as_deref() != Some(root.as_path()) {
        settings.last_root = Some(root);
        match settings.save() {
            Ok(()) => app.settings().set(settings),
            Err(e) => toast(app, ToastKind::Error, format!("settings not saved: {e}")),
        }
    }
}

/// Every `config.toml` under `root` with its estates: the walk, the config parse and
/// the params of each estate, all on a blocking thread.
fn walk(root: &Path) -> Vec<EstateSummary> {
    EstateDir::discover(root)
        .into_iter()
        .map(|config| match EstateDir::open(&config) {
            Ok(dir) => {
                let (estates, error) = match dir.estates() {
                    Ok(paths) => (
                        paths.into_iter().map(|p| estate_file(&dir, p)).collect(),
                        None,
                    ),
                    Err(e) => (Vec::new(), Some(e.to_string())),
                };
                EstateSummary {
                    config,
                    dir: dir.dir.clone(),
                    estates,
                    error,
                }
            }
            Err(e) => EstateSummary {
                dir: config.parent().map(Path::to_path_buf).unwrap_or_default(),
                config,
                estates: Vec::new(),
                error: Some(e.to_string()),
            },
        })
        .collect()
}

fn estate_file(dir: &EstateDir, path: PathBuf) -> EstateFile {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let deployment_mode = dir.deployment_mode(&path).map_err(|e| e.to_string());
    EstateFile {
        path,
        name,
        deployment_mode,
    }
}

async fn open_estate(app: Store<AppStore>, config: PathBuf, estate: PathBuf) {
    let Some(bin) = app.satz().read().binary().cloned() else {
        toast(
            app,
            ToastKind::Error,
            "satz is not available — see the banner",
        );
        return;
    };
    if app.opening().read().is_some() {
        toast(app, ToastKind::Info, "an estate is already being opened");
        return;
    }
    app.opening().set(Some(estate.clone()));
    let allow = app.settings().read().mcp_allow;
    let opened = match EstateDir::open(&config) {
        Ok(dir) => EstateSession::open(&bin, dir, estate.clone(), allow)
            .await
            .map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    match opened {
        Ok(session) => {
            close_estate(app);
            let name = estate
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let runs_as = session.runs_as().map(str::to_string);
            let deployment_mode = session.deployment_mode().map(str::to_string);
            let dir = session.dir.dir.clone();
            let identity = runs_as
                .clone()
                .unwrap_or_else(|| "the ADC identity".to_string());
            app.open().set(Some(OpenEstate {
                session: Arc::clone(&session),
                dir,
                main: estate,
                name: name.clone(),
                runs_as,
                deployment_mode,
            }));
            toast(app, ToastKind::Info, format!("Opened {name} as {identity}"));
        }
        Err(e) => toast(app, ToastKind::Error, format!("{}: {e}", estate.display())),
    }
    app.opening().set(None);
}

/// The `satz init` run as it reads on a command line: the directory it runs in, then
/// the command, because the working directory is the whole of the address — there is no
/// `--config` on this call and no `config.toml` yet for one to name. An empty `dir` is
/// the form before a folder has been chosen and yields the command alone.
pub fn create_command_line(dir: &Path, args: &[String]) -> String {
    let command: Vec<String> = std::iter::once("satz".to_string())
        .chain(args.iter().map(|a| quote(a)))
        .collect();
    let command = command.join(" ");
    if dir.as_os_str().is_empty() {
        return command;
    }
    format!("cd {} && {command}", quote(&dir.display().to_string()))
}

/// `satz init` in `dir`. The target is checked first, the run streams into the create
/// log, and what it made is READ from the directory afterwards — `init` names the estate
/// file after a customer id it may have derived from the credentials, so the name is
/// never predicted here.
///
/// Nothing the run derived leaves this function: the lines go into `create.log`, which
/// lives as long as the window shows it, and the values themselves are in the estate
/// satz wrote.
fn create_estate(
    app: Store<AppStore>,
    dir: PathBuf,
    options: InitOptions,
) -> Option<CancellationToken> {
    if app.create().running().cloned() {
        toast(app, ToastKind::Info, "an estate is already being created");
        return None;
    }
    let Some(bin) = app.satz().read().binary().cloned() else {
        toast(
            app,
            ToastKind::Error,
            "satz is not available — see the banner",
        );
        return None;
    };
    if let Err(e) = satz_studio_core::satz::init::check_target(&dir) {
        toast(app, ToastKind::Error, e.to_string());
        return None;
    }

    let args = options.argv();
    let token = CancellationToken::new();
    let (tx, mut lines) = tokio::sync::mpsc::channel::<CliLine>(256);
    let child = token.clone();
    let argv = args.clone();
    let run_in = dir.clone();
    let join =
        tokio::spawn(async move { SatzCli::run_in(&bin.path, &run_in, &argv, tx, child).await });

    let create = app.create();
    create.command().set(Some(create_command_line(&dir, &args)));
    create.log().clear();
    create.outcome().set(None);
    create.running().set(true);
    spawn(async move {
        let mut last_stderr = None;
        while let Some(line) = lines.recv().await {
            let clean = match line {
                CliLine::Stdout(s) => CliLine::Stdout(strip_ansi(&s)),
                CliLine::Stderr(s) => {
                    let s = strip_ansi(&s);
                    if !s.trim().is_empty() {
                        last_stderr = Some(s.clone());
                    }
                    CliLine::Stderr(s)
                }
            };
            create.log().push(clean);
        }
        let ended = join.await;
        // a cancel is what the user asked for, so it is the outcome chip and not a toast
        let cancelled = matches!(ended, Ok(Err(SatzError::Cancelled)));
        let outcome = match ended {
            // satz said why on its own stderr; a status line alone would replace that
            // sentence with a number
            Ok(Ok(status)) if !status.success() => Some(CommandOutcome {
                ok: false,
                text: last_stderr.unwrap_or_else(|| format!("exited with {status}")),
            }),
            Ok(Ok(_)) => None,
            Ok(Err(SatzError::Cancelled)) => Some(CommandOutcome {
                ok: false,
                text: "cancelled — the directory is left as satz left it".to_string(),
            }),
            Ok(Err(e)) => Some(CommandOutcome {
                ok: false,
                text: e.to_string(),
            }),
            Err(e) => Some(CommandOutcome {
                ok: false,
                text: format!("the create task failed: {e}"),
            }),
        };
        let outcome = match outcome {
            Some(failed) => failed,
            None => created(app, &dir).await,
        };
        if !outcome.ok && !cancelled {
            toast(app, ToastKind::Error, outcome.text.clone());
        }
        create.outcome().set(Some(outcome));
        create.running().set(false);
    });
    Some(token)
}

/// What the finished run left in `dir`, and the estate opened when it made exactly one.
async fn created(app: Store<AppStore>, dir: &Path) -> CommandOutcome {
    let read = dir.to_path_buf();
    let found = tokio::task::spawn_blocking(move || {
        satz_studio_core::satz::init::created(&read).map_err(|e| e.to_string())
    })
    .await;
    let (estate_dir, estates) = match found {
        Ok(Ok(found)) => found,
        Ok(Err(e)) => return CommandOutcome { ok: false, text: e },
        Err(e) => {
            return CommandOutcome {
                ok: false,
                text: format!("reading {}: {e}", dir.display()),
            };
        }
    };
    match estates.as_slice() {
        // satz writes the directories and the config whatever happens, and the estate
        // file only once it has a customer id — stated, or derived from the credentials
        [] => CommandOutcome {
            ok: false,
            text: format!(
                "satz wrote no estate file in {}: no customer id was stated and none could be derived. The log has what satz said.",
                dir.display()
            ),
        },
        [estate] => {
            let name = estate
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            open_estate(app, estate_dir.config_path.clone(), estate.clone()).await;
            CommandOutcome {
                ok: true,
                text: format!("created {name}"),
            }
        }
        many => CommandOutcome {
            ok: false,
            text: format!(
                "{} estate files in {} — open the one you want",
                many.len(),
                dir.display()
            ),
        },
    }
}

/// Write the settings file, put them in the store and locate satz again. The one
/// saver: the Settings view sends [`AppAction::SaveSettings`], and the Chat view's
/// "Use Claude Code" button awaits this directly, because it must not rebuild the
/// engine on settings that were not saved. A file that did not write is a toast and
/// the sentence, so the caller can refuse to go on.
pub async fn save_settings(app: Store<AppStore>, settings: Settings) -> Result<(), String> {
    match settings.save() {
        Ok(()) => {
            app.settings().set(settings);
            toast(app, ToastKind::Info, "Settings saved");
            locate(app).await;
            Ok(())
        }
        Err(e) => {
            let message = format!("settings not saved: {e}");
            toast(app, ToastKind::Error, message.clone());
            Err(message)
        }
    }
}

async fn resolve_credential(app: Store<AppStore>) {
    let status = match Credential::resolve().await {
        Ok((_, source)) => CredentialStatus::Resolved(source),
        Err(e) => CredentialStatus::Error(e.to_string()),
    };
    app.credential().set(status);
}

async fn store_key(app: Store<AppStore>, key: String) {
    let stored = tokio::task::spawn_blocking(move || {
        Credential::store_in_keychain(&key).map_err(|e| e.to_string())
    })
    .await;
    match stored {
        Ok(Ok(())) => {
            toast(app, ToastKind::Info, "Key stored in the keychain");
            resolve_credential(app).await;
        }
        Ok(Err(e)) => toast(app, ToastKind::Error, format!("key not stored: {e}")),
        Err(e) => toast(app, ToastKind::Error, format!("key not stored: {e}")),
    }
}
