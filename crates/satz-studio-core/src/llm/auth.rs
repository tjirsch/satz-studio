//! Credentials. satz-studio owns no OAuth client and writes no key to disk: a
//! credential comes from the environment, from the `ant` CLI's profile, or from the
//! OS keychain entry the Settings view stores (`satz-studio` / `anthropic-api-key`).

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::claude::error::ClaudeError;

pub const KEYCHAIN_SERVICE: &str = "satz-studio";
pub const KEYCHAIN_USER: &str = "anthropic-api-key";
/// How long `ant auth print-credentials` may take before it is treated as absent.
const ANT_TIMEOUT: Duration = Duration::from_secs(15);

/// How a request authenticates. `Debug` never prints the secret.
#[derive(Clone, PartialEq, Eq)]
pub enum Credential {
    /// `x-api-key`
    ApiKey(String),
    /// `Authorization: Bearer` + the OAuth beta header — from `ANTHROPIC_AUTH_TOKEN` or `ant auth`
    Bearer(String),
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Credential::ApiKey(_) => f.write_str("ApiKey(…)"),
            Credential::Bearer(_) => f.write_str("Bearer(…)"),
        }
    }
}

/// Where a credential came from — shown in Settings so a stale key is visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    ApiKeyEnv,
    AuthTokenEnv,
    AntProfile,
    Keychain,
}

impl Credential {
    /// `ANTHROPIC_API_KEY`, then `ANTHROPIC_AUTH_TOKEN`, then the `ant auth login`
    /// profile (`ant auth print-credentials --access-token`, when `ant` is on `PATH`),
    /// then the keychain entry the Settings view writes. First hit wins; an empty
    /// variable is unset. [`ClaudeError::NoCredential`] names what every source
    /// answered; a keychain that cannot be opened is [`ClaudeError::Keychain`].
    pub async fn resolve() -> Result<(Credential, CredentialSource), ClaudeError> {
        let mut tried = Vec::new();
        match non_empty_env("ANTHROPIC_API_KEY") {
            Some(key) => return Ok((Credential::ApiKey(key), CredentialSource::ApiKeyEnv)),
            None => tried.push("ANTHROPIC_API_KEY is not set".to_string()),
        }
        match non_empty_env("ANTHROPIC_AUTH_TOKEN") {
            Some(token) => return Ok((Credential::Bearer(token), CredentialSource::AuthTokenEnv)),
            None => tried.push("ANTHROPIC_AUTH_TOKEN is not set".to_string()),
        }
        match which::which("ant") {
            Ok(ant) => match ant_access_token(&ant).await {
                Ok(token) => return Ok((Credential::Bearer(token), CredentialSource::AntProfile)),
                Err(note) => tried.push(note),
            },
            Err(_) => tried.push("`ant` is not on PATH".to_string()),
        }
        match tokio::task::spawn_blocking(read_keychain).await.map_err(|e| ClaudeError::Keychain(format!("the keychain task failed: {e}")))?? {
            Some(key) => return Ok((Credential::ApiKey(key), CredentialSource::Keychain)),
            None => tried.push(format!("no keychain entry {KEYCHAIN_SERVICE} / {KEYCHAIN_USER}")),
        }
        Err(ClaudeError::NoCredential { tried })
    }

    /// Store a key in the OS keychain (`satz-studio` / `anthropic-api-key`).
    pub fn store_in_keychain(key: &str) -> Result<(), ClaudeError> {
        if key.trim().is_empty() {
            return Err(ClaudeError::Keychain("an empty key is not stored".to_string()));
        }
        keychain_entry()?.set_password(key).map_err(|e| ClaudeError::Keychain(e.to_string()))
    }

    /// Remove the keychain entry. An entry that does not exist is already removed.
    pub fn delete_from_keychain() -> Result<(), ClaudeError> {
        match keychain_entry()?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(ClaudeError::Keychain(e.to_string())),
        }
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// The access token of the active `ant` profile; a note saying why not, otherwise.
async fn ant_access_token(ant: &std::path::Path) -> Result<String, String> {
    let output = tokio::time::timeout(ANT_TIMEOUT, tokio::process::Command::new(ant).args(["auth", "print-credentials", "--access-token"]).output())
        .await
        .map_err(|_| format!("`ant auth print-credentials` did not answer within {}s", ANT_TIMEOUT.as_secs()))?
        .map_err(|e| format!("`ant auth print-credentials` could not run: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("`ant auth print-credentials` exited with {}: {stderr}", output.status));
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        return Err("`ant auth print-credentials` printed no token".to_string());
    }
    Ok(token)
}

/// The key in the keychain, `None` when there is no entry.
fn read_keychain() -> Result<Option<String>, ClaudeError> {
    match keychain_entry()?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(e) => Err(ClaudeError::Keychain(e.to_string())),
    }
}

fn keychain_entry() -> Result<keyring_core::Entry, ClaudeError> {
    register_store()?;
    keyring_core::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_USER).map_err(|e| ClaudeError::Keychain(e.to_string()))
}

static STORE: OnceLock<Result<(), String>> = OnceLock::new();

/// Register the platform's credential store as keyring-core's default, once per process.
fn register_store() -> Result<(), ClaudeError> {
    STORE
        .get_or_init(|| {
            let store = platform_store().map_err(|e| e.to_string())?;
            keyring_core::set_default_store(store);
            Ok(())
        })
        .clone()
        .map_err(ClaudeError::Keychain)
}

#[cfg(target_os = "macos")]
fn platform_store() -> keyring_core::Result<Arc<keyring_core::CredentialStore>> {
    apple_native_keyring_store::keychain::Store::new().map(|s| s as Arc<keyring_core::CredentialStore>)
}

#[cfg(target_os = "windows")]
fn platform_store() -> keyring_core::Result<Arc<keyring_core::CredentialStore>> {
    windows_native_keyring_store::Store::new().map(|s| s as Arc<keyring_core::CredentialStore>)
}

#[cfg(target_os = "linux")]
fn platform_store() -> keyring_core::Result<Arc<keyring_core::CredentialStore>> {
    zbus_secret_service_keyring_store::Store::new().map(|s| s as Arc<keyring_core::CredentialStore>)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn platform_store() -> keyring_core::Result<Arc<keyring_core::CredentialStore>> {
    Err(keyring_core::Error::NotSupportedByStore("no keychain store is built for this platform".to_string()))
}
