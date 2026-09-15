//! The app coroutine: satz, the estate folder, sessions, settings and the credential.
//! It runs for the life of the window; the views send [`AppAction`]s and read the
//! store.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dioxus::prelude::*;
use futures_util::StreamExt;
use satz_studio_core::estate::EstateDir;
use satz_studio_core::llm::Credential;
use satz_studio_core::satz::{EstateSession, SatzBinary, SatzError};
use satz_studio_core::settings::Settings;

use super::{
    AppStore, AppStoreStoreExt, CredentialStatus, EstateFile, EstateStore, EstateSummary,
    OpenEstate, SatzStatus, ToastKind, toast,
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
    while let Some(action) = rx.next().await {
        match action {
            AppAction::LocateSatz => locate(app).await,
            AppAction::Discover(root) => discover(app, root).await,
            AppAction::OpenEstate { config, estate } => open_estate(app, config, estate).await,
            AppAction::CloseEstate => close_estate(app),
            // the toast said what went wrong; the view keeps the draft either way
            AppAction::SaveSettings(settings) => {
                let _ = save_settings(app, settings).await;
            }
            AppAction::ResolveCredential => resolve_credential(app).await,
            AppAction::StoreKey(key) => store_key(app, key).await,
        }
    }
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
        Err(SatzError::TooOld { found, required }) => SatzStatus::TooOld {
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
