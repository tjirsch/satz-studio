//! The app coroutine: satz, the estate folder, sessions, settings and the credential.
//! It runs for the life of the window; the views send [`AppAction`]s and read the
//! store.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dioxus::prelude::*;
use futures_util::StreamExt;
use satz_studio_core::estate::EstateDir;
use satz_studio_core::github;
use satz_studio_core::llm::Credential;
use satz_studio_core::satz::import::{ImportPlan, satz_files, written_since};
use satz_studio_core::satz::self_update::{read_check, unprompted_checks_allowed};
use satz_studio_core::satz::{
    CliLine, EstateSession, ImportOptions, ImportReport, InitOptions, SatzBinary, SatzCli,
    SatzError,
};
use satz_studio_core::settings::Settings;
use tokio_util::sync::CancellationToken;

use super::install;
use super::{
    AppStore, AppStoreStoreExt, CommandOutcome, CreateStoreStoreExt, CredentialStatus, EstateFile,
    EstateStore, EstateSummary, ImportStoreStoreExt, OpenEstate, SatzStatus,
    StudioLookStoreStoreExt, ToastKind, UpdateStoreStoreExt, View, quote, satz_release_sentence,
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
    /// `satz import` in `dir`, streamed into the import log, preceded by `satz init`
    /// when `dir` holds no `config.toml` yet; the estate the import wrote is opened
    ImportEstate {
        dir: PathBuf,
        options: ImportOptions,
        /// what the `satz init` half runs with, used only when `dir` is not an estate
        /// yet — the directory decides, never the form
        init: InitOptions,
    },
    CancelImport,
    /// `satz self-update` on the satz that is installed, streamed into the update log,
    /// followed by locating satz again so the new version is the one in use. satz owns
    /// its own updater; the app only runs it. `check_only` passes `--check-only`, which
    /// reports without installing. On Windows, where `satz self-update` does not install,
    /// the update is satz's PowerShell installer, verified and run into the folder of the
    /// satz in use
    UpdateSatz {
        check_only: bool,
    },
    CancelUpdate,
    /// hide the notice for a satz newer than the build, for that satz version: the
    /// version goes into `Settings.dismissed_satz`, and the notice comes back for any other
    /// newer release. It permits and refuses nothing
    DismissSatzNotice(String),
    /// read the latest satz-studio release on GitHub and compare it with this build — a
    /// look, never an update: nothing is downloaded, run or written. The app looks once at
    /// launch on its own; this is the look again, when asked
    LookForStudioUpdate,
    /// satz's own installer — the shell script, or the PowerShell script on Windows —
    /// verified against its SHA-256 sidecar and run into `~/.local/bin` without editing the
    /// `PATH` or the shell profile, while no satz is found; then satz is located again
    InstallSatz,
    CancelInstall,
    /// write the settings file, then locate satz again
    SaveSettings(Settings),
    ResolveCredential,
    /// put a key in the OS keychain, then resolve again
    StoreKey(String),
}

pub async fn app_coroutine(mut rx: UnboundedReceiver<AppAction>, app: Store<AppStore>) {
    locate(app).await;
    // the two release looks run from tasks of their own, so the walk does not wait on them
    let mut updating: Option<CancellationToken> = look_at_launch(app);
    if let Some(root) = app.root().cloned() {
        discover(app, root).await;
    }
    // `init` is a live command against Google and can take a while; it streams from a
    // task so this loop stays free to take `CancelCreate`, as a command does.
    let mut creating: Option<CancellationToken> = None;
    let mut importing: Option<CancellationToken> = None;
    let mut installing: Option<CancellationToken> = None;
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
            AppAction::ImportEstate { dir, options, init } => {
                importing = import_estate(app, dir, options, init);
            }
            AppAction::CancelImport => {
                if let Some(token) = importing.take() {
                    token.cancel();
                }
            }
            AppAction::UpdateSatz { check_only } => {
                if let Some(token) = update_satz(app, check_only, Asked::ByOperator) {
                    updating = Some(token);
                }
            }
            AppAction::CancelUpdate => {
                if let Some(token) = updating.take() {
                    token.cancel();
                }
            }
            AppAction::DismissSatzNotice(version) => dismiss_satz_notice(app, version).await,
            AppAction::LookForStudioUpdate => look_for_studio_update(app),
            AppAction::InstallSatz => {
                installing = install::install_satz(app);
            }
            AppAction::CancelInstall => {
                if let Some(token) = installing.take() {
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
/// On Windows `satz self-update` checks but does not install, so an update there is
/// [`install::update_by_installer`]: satz's PowerShell installer, verified against its
/// sidecar and run into the folder of that same binary.
///
/// A `check_only` run is read: satz's `Latest version:` line against the satz that was
/// asked becomes `update.found`, which the top bar, the title and Settings offer. An
/// install clears it. A run the app started on its own ([`Asked::AtLaunch`]) says a failure
/// in the log card and never in a toast: a look that could not reach GitHub is a fact to
/// read, not an error to interrupt with.
///
/// The operator's `self_update_frequency` is theirs; this writes no satz configuration.
fn update_satz(app: Store<AppStore>, check_only: bool, asked: Asked) -> Option<CancellationToken> {
    if app.update().running().cloned() {
        if asked == Asked::ByOperator {
            toast(app, ToastKind::Info, "satz self-update is already running");
        }
        return None;
    }
    let Some(path) = app.satz().read().updatable().map(Path::to_path_buf) else {
        if asked == Asked::ByOperator {
            toast(
                app,
                ToastKind::Error,
                "there is no satz to update — see the banner",
            );
        }
        return None;
    };
    // satz's `self-update` refuses to install on Windows and names its PowerShell
    // installer; the check alone runs there as everywhere
    if cfg!(windows) && !check_only {
        return install::update_by_installer(app, &path);
    }
    // the version of the satz asked, which the check's answer is compared with
    let current = match &*app.satz().read() {
        SatzStatus::Located(bin) => Some(bin.version.clone()),
        SatzStatus::TooOld { found, .. } => semver::Version::parse(found).ok(),
        _ => None,
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
        let mut stdout = String::new();
        while let Some(line) = lines.recv().await {
            let clean = match line {
                CliLine::Stdout(s) => {
                    let s = strip_ansi(&s);
                    stdout.push_str(&s);
                    stdout.push('\n');
                    CliLine::Stdout(s)
                }
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
            Ok(Ok(_)) if check_only => match &current {
                Some(current) => match read_check(&stdout, current) {
                    Ok(release) => {
                        let found = Ok(release);
                        let text = satz_release_sentence(&found, current);
                        app.update().found().set(Some(found));
                        CommandOutcome { ok: true, text }
                    }
                    Err(e) => CommandOutcome { ok: false, text: e },
                },
                None => CommandOutcome {
                    ok: true,
                    text: "checked — the log has what satz found".to_string(),
                },
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
        // a check that failed is what the look found this session, with its reason
        if check_only && !outcome.ok && !cancelled {
            app.update().found().set(Some(Err(outcome.text.clone())));
        }
        if !outcome.ok && !cancelled && asked == Asked::ByOperator {
            toast(app, ToastKind::Error, outcome.text.clone());
        }
        let installed = outcome.ok && !check_only;
        app.update().outcome().set(Some(outcome));
        app.update().running().set(false);
        // The version on disk changed, so the one the app holds is stale, and so is what a
        // check found against the old one.
        if installed {
            app.update().found().set(None);
            locate(app).await;
        }
    });
    Some(token)
}

/// Who started a `satz self-update` run.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Asked {
    /// the operator, from the banner, the top bar or Settings: a failure is a toast
    ByOperator,
    /// the app, once at launch: a failure is read in Settings and interrupts nothing
    AtLaunch,
}

/// The two release looks the app makes on its own, once per launch and never on a timer:
/// the latest satz-studio release against this build, and `satz self-update --check-only`
/// on the satz that runs. Their results are kept for the session.
///
/// satz is asked only when there is a satz, and only when the operator's own satz config
/// lets satz look for releases unprompted: `self_update_frequency = "never"` is read as
/// "not on my behalf either", and the look then waits for "Check only". A satz config that
/// does not parse is said the same way — satz refuses to run with it too.
fn look_at_launch(app: Store<AppStore>) -> Option<CancellationToken> {
    look_for_studio_update(app);
    app.satz().read().updatable()?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match unprompted_checks_allowed(home.as_deref()) {
        Ok(true) => update_satz(app, true, Asked::AtLaunch),
        Ok(false) => {
            app.update().not_checked().set(Some(
                "satz was not asked for a newer release at launch: your satz config says self_update_frequency = \"never\". Check only asks it.".to_string(),
            ));
            None
        }
        Err(e) => {
            app.update().not_checked().set(Some(format!(
                "satz was not asked for a newer release at launch: {e}"
            )));
            None
        }
    }
}

/// Drop the session: the estate host unmounts and its coroutine with it.
/// Closing an estate is how estates are SWITCHED: the window has nowhere to stand
/// without one, so it goes back to the Start screen with its doors, and the palette
/// over it — which acts on the estate that is gone — closes with it.
pub fn close_estate(app: Store<AppStore>) {
    app.open().set(None);
    app.estate().set(EstateStore::default());
    app.palette_open().set(false);
    app.nav().set(View::Start);
}

/// Find satz. The driver refuses an older satz and locates a newer one, which runs: the
/// notice for it is read from the status (`satz_notice`), not decided here.
pub(super) async fn locate(app: Store<AppStore>) {
    let override_path = app.settings().cloned().satz_binary;
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
        Err(e @ SatzError::NotFound { .. }) => SatzStatus::Missing(e.to_string()),
        Err(e) => SatzStatus::Unusable(e.to_string()),
    };
    tracing::info!(?status, "satz located");
    app.satz().set(status);
}

/// Hide the notice for a satz newer than the build, for that version. The version goes
/// into the settings file; nothing is located again, because nothing about which satz runs
/// has changed.
async fn dismiss_satz_notice(app: Store<AppStore>, version: String) {
    let mut settings = app.settings().cloned();
    settings.dismissed_satz = Some(version);
    match settings.save() {
        Ok(()) => app.settings().set(settings),
        Err(e) => toast(app, ToastKind::Error, format!("settings not saved: {e}")),
    }
}

/// The latest satz-studio release, compared with this build, on a task of its own so the
/// app coroutine stays free. One look at a time; a look that fails keeps its reason in
/// `studio_look` and raises no toast.
fn look_for_studio_update(app: Store<AppStore>) {
    let look = app.studio_look();
    if look.looking().cloned() {
        return;
    }
    look.outcome().set(None);
    look.looking().set(true);
    spawn(async move {
        let running = semver::Version::parse(env!("CARGO_PKG_VERSION"))
            .expect("the package version is a version");
        let found = github::look_for_studio_update(&github::client(), github::API, &running)
            .await
            .map_err(|e| e.to_string());
        tracing::info!(?found, "looked for a satz-studio update");
        app.studio_look().outcome().set(Some(found));
        app.studio_look().looking().set(false);
    });
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
            // An estate that opens lands on its Overview: what it still owes is the
            // first thing to read, whichever door it came through.
            app.nav().set(View::Overview);
            toast(app, ToastKind::Info, format!("Opened {name} as {identity}"));
        }
        Err(e) => toast(app, ToastKind::Error, format!("{}: {e}", estate.display())),
    }
    app.opening().set(None);
}

/// A run IN a directory as it reads on a command line: the directory it runs in, then
/// the command, because the working directory is the whole of the address — there is no
/// `--config` on these calls and, before `init`, no `config.toml` yet for one to name.
/// An empty `dir` is a form before a folder has been chosen and yields the command
/// alone. Create and Import both preview their runs with it.
pub fn run_line(dir: &Path, args: &[String]) -> String {
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
    create.command().set(Some(run_line(&dir, &args)));
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
            // A created estate is a skeleton: every question of its own is open and it
            // owes them all, so it lands on Decisions rather than on an Overview that
            // would only say so. The rail keeps one order for every estate — where the
            // window LANDS is what the door decides, not what the rail reads.
            if app.open().read().is_some() {
                app.nav().set(View::Decisions);
            }
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

// ---- the Import door -----------------------------------------------------------
//
// `satz import` imports INTO a project — run where there is no `config.toml` it refuses
// and creates nothing. So the door is two steps, and the DIRECTORY decides which:
// `ImportPlan` is read from disk in the runner, never taken from the form, so a form
// filled in before the folder changed cannot run an `init` over an estate or skip one
// that is needed.
//
// What the run wrote is read back rather than predicted: the file name differs by shape
// and `--output` moves it again, so the `.satz` files of the directories the shape writes
// into are hashed before the import and compared after. Exactly one of them declaring an
// estate is the estate that opens; none at all is a failure, whatever the exit status
// said.
//
// Nothing the run derived leaves here. A live import prints the organisation id, the
// customer directory id, the billing account and an administrator's address it read from
// the credentials: those lines go into the import log, which lives as long as the window
// shows it, and the values themselves are in the estate satz wrote.

/// `satz import` in `dir`, with `satz init` first when `dir` holds no `config.toml`.
///
/// Everything that can refuse before a child is spawned does: satz is there, no run is in
/// flight, the directory exists, and the source is one this shape can read — which is
/// where a raw `.tfstate` is turned away with satz's own sentence rather than after a
/// run.
pub fn import_estate(
    app: Store<AppStore>,
    dir: PathBuf,
    options: ImportOptions,
    init: InitOptions,
) -> Option<CancellationToken> {
    if app.import().running().cloned() {
        toast(app, ToastKind::Info, "an import is already running");
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
    let plan = match satz_studio_core::satz::import::plan(&dir) {
        Ok(plan) => plan,
        Err(e) => {
            toast(app, ToastKind::Error, e.to_string());
            return None;
        }
    };
    if let Err(e) = options.check_source(&dir) {
        toast(app, ToastKind::Error, e.to_string());
        return None;
    }

    let init_args = init.argv();
    let import_args = options.argv();
    let command = match plan {
        ImportPlan::Import => run_line(&dir, &import_args),
        ImportPlan::InitThenImport => format!(
            "{}\n{}",
            run_line(&dir, &init_args),
            run_line(&dir, &import_args)
        ),
    };

    let token = CancellationToken::new();
    let store = app.import();
    store.command().set(Some(command));
    store.log().clear();
    store.outcome().set(None);
    store.report().set(ImportReport::default());
    store.running().set(true);

    let satz = bin.path.clone();
    let child = token.clone();
    spawn(async move {
        let (outcome, cancelled) = run_import(app, satz, dir, plan, init, options, child).await;
        // a cancel is what the user asked for, so it is the outcome chip and not a toast
        if !outcome.ok && !cancelled {
            toast(app, ToastKind::Error, outcome.text.clone());
        }
        app.import().outcome().set(Some(outcome));
        app.import().running().set(false);
    });
    Some(token)
}

/// The sequence: `init` when the plan says so, the directories read before the import,
/// the import, its report, and the estate it wrote. The second half of the answer is
/// whether the operator cancelled, which is an outcome and not an error to raise.
async fn run_import(
    app: Store<AppStore>,
    satz: PathBuf,
    dir: PathBuf,
    plan: ImportPlan,
    init: InitOptions,
    options: ImportOptions,
    cancel: CancellationToken,
) -> (CommandOutcome, bool) {
    let failed = |text: String| (CommandOutcome { ok: false, text }, false);
    if plan == ImportPlan::InitThenImport {
        let end = stream_into_import_log(app, &satz, &dir, &init.argv(), cancel.clone()).await;
        if let Some(failure) = end.failure {
            if failure.cancelled {
                return (
                    CommandOutcome {
                        ok: false,
                        text: failure.text,
                    },
                    true,
                );
            }
            // the import never ran, and on this path that is the thing to say first
            return failed(format!(
                "satz init refused, so nothing was imported: {}",
                failure.text
            ));
        }
    }

    // `init` has written config.toml by now, whichever way this got here
    let estate = match EstateDir::open(&dir) {
        Ok(estate) => estate,
        Err(e) => return failed(e.to_string()),
    };
    let dirs = options.write_dirs(&estate);
    let before = match read_import_dirs(dirs.clone(), satz_files).await {
        Ok(before) => before,
        Err(text) => return failed(text),
    };

    let end = stream_into_import_log(app, &satz, &dir, &options.argv(), cancel).await;
    app.import().report().set(ImportReport::of(&end.lines));
    if let Some(failure) = end.failure {
        return (
            CommandOutcome {
                ok: false,
                text: failure.text,
            },
            failure.cancelled,
        );
    }

    let written =
        match read_import_dirs(dirs.clone(), move |dirs| written_since(&before, dirs)).await {
            Ok(written) => written,
            Err(text) => return failed(text),
        };
    let estates: Vec<PathBuf> = written
        .iter()
        .filter(|w| w.declares_estate)
        .map(|w| w.path.clone())
        .collect();
    let names = |paths: &[PathBuf]| {
        paths
            .iter()
            .map(|p| {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let done = |text: String| (CommandOutcome { ok: true, text }, false);
    match (written.len(), estates.len()) {
        // satz exited zero and left nothing behind: the log says what it did, and this
        // is not an import that quietly succeeded
        (0, _) => failed(format!(
            "satz wrote no .satz file in {} — the report and the log have what it said",
            dirs.iter()
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
        (_, 1) => {
            let estate_file = estates[0].clone();
            let name = estate_file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            open_estate(app, estate.config_path.clone(), estate_file).await;
            done(format!("imported {name}"))
        }
        // a converted pack is a file, not an estate: there is nothing to open, and
        // saying so is better than opening something else
        (_, 0) => done(format!(
            "wrote {} — no estate is declared there, so nothing was opened",
            names(&written.iter().map(|w| w.path.clone()).collect::<Vec<_>>())
        )),
        (_, many) => done(format!(
            "{many} estate files written ({}) — open the one you want",
            names(&estates)
        )),
    }
}

/// The import's write directories read on a blocking thread, with the failure as the
/// sentence it will be shown as.
async fn read_import_dirs<T, F>(dirs: Vec<PathBuf>, read: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&[PathBuf]) -> Result<T, satz_studio_core::estate::EstateError> + Send + 'static,
{
    match tokio::task::spawn_blocking(move || read(&dirs)).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(e)) => Err(e.to_string()),
        Err(e) => Err(format!("reading what the import wrote: {e}")),
    }
}

/// How one command of the sequence ended.
struct RunEnd {
    /// why it did not finish. `None` is a clean exit.
    failure: Option<Failure>,
    /// every line it streamed, which is what the import report is read out of
    lines: Vec<CliLine>,
}

/// A command that did not finish: what to show, and whether the operator asked for it.
struct Failure {
    /// satz's own last stderr line, the cancel, or the status
    text: String,
    cancelled: bool,
}

/// One satz command in `dir`, streamed into the import log and collected.
async fn stream_into_import_log(
    app: Store<AppStore>,
    satz: &Path,
    dir: &Path,
    args: &[String],
    cancel: CancellationToken,
) -> RunEnd {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<CliLine>(256);
    let (satz, dir, argv) = (satz.to_path_buf(), dir.to_path_buf(), args.to_vec());
    let join = tokio::spawn(async move { SatzCli::run_in(&satz, &dir, &argv, tx, cancel).await });

    let store = app.import();
    let mut lines = Vec::new();
    let mut last_stderr = None;
    while let Some(line) = rx.recv().await {
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
        store.log().push(clean.clone());
        lines.push(clean);
    }
    let refused = |text: String| {
        Some(Failure {
            text,
            cancelled: false,
        })
    };
    let failure = match join.await {
        // satz said why on its own stderr; a status line alone would replace that
        // sentence with a number
        Ok(Ok(status)) if !status.success() => {
            refused(last_stderr.unwrap_or_else(|| format!("exited with {status}")))
        }
        Ok(Ok(_)) => None,
        Ok(Err(SatzError::Cancelled)) => Some(Failure {
            text: "cancelled — the folder is left as satz left it".to_string(),
            cancelled: true,
        }),
        Ok(Err(e)) => refused(e.to_string()),
        Err(e) => refused(format!("the import task failed: {e}")),
    };
    RunEnd { failure, lines }
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
