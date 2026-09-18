//! satz's own installer, run for an operator who has no satz: the cargo-dist shell
//! installer of the latest satz release, verified against its SHA-256 sidecar before a
//! byte of it runs.
//!
//! - **One release object.** The installer and the sidecar are the assets of the release
//!   `releases/latest` names at that moment ([`crate::github::latest_release`]), never two
//!   separate `latest/download` URLs, which a release published in between would pair
//!   across two releases.
//! - **Verified before it runs.** A [`VerifiedInstaller`] exists only once the script's
//!   SHA-256 equals the sidecar's; a release without a sidecar, a sidecar that is not a
//!   SHA-256 and a mismatch are each a refusal, and nothing runs.
//! - **No edit of the shell profile.** The installer adds `~/.local/bin` to `PATH` in the
//!   operator's shell profiles unless `SATZ_NO_MODIFY_PATH=1` is in its environment, and
//!   the app sets it: `SatzBinary::locate` searches `~/.local/bin/satz` itself, so the app
//!   needs no `PATH` change and makes none.
//! - **Nothing to answer.** The installer asks nothing; its stdin is closed all the same,
//!   so a prompt in some later version would read end-of-file rather than wait.
//! - **Not on Windows.** satz's Windows installer is `satz-installer.ps1`, PowerShell,
//!   which this module does not run; there nothing is attempted and the refusal names the
//!   one-liner that installs it.

#[cfg(not(windows))]
use std::process::{ExitStatus, Stdio};

use sha2::{Digest, Sha256};
#[cfg(not(windows))]
use tokio::sync::mpsc;
#[cfg(not(windows))]
use tokio_util::sync::CancellationToken;

#[cfg(not(windows))]
use super::{CliLine, SatzError};
use crate::github::{self, GithubError};

/// satz's repository, whose latest release is installed.
pub const SATZ_REPO: &str = "tjirsch/satz";
/// The cargo-dist shell installer asset.
pub const INSTALLER: &str = "satz-installer.sh";
/// The checksum satz's `attach-checksum` job attaches to every release: `sha256sum`'s
/// output, `<hex>  satz-installer.sh`.
pub const SIDECAR: &str = "satz-installer.sh.sha256";

/// The installer's switch for leaving the shell profiles alone.
pub const NO_MODIFY_PATH: (&str, &str) = ("SATZ_NO_MODIFY_PATH", "1");

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error(
        "satz-studio does not install satz on Windows yet: satz publishes a Windows build since 0.63.0, but through a PowerShell installer this app does not run. Install it with `powershell -ExecutionPolicy Bypass -c \"irm https://github.com/tjirsch/satz/releases/latest/download/satz-installer.ps1 | iex\"`, or set the path to satz.exe in Settings"
    )]
    NoWindowsInstall,
    #[error(transparent)]
    Github(#[from] GithubError),
    #[error("the satz release {release} has no {INSTALLER} — its release build has not finished")]
    NoInstaller { release: String },
    #[error(
        "the satz release {release} has no {SIDECAR}, so its installer cannot be verified and is not run"
    )]
    NoSidecar { release: String },
    #[error("{SIDECAR} is not a SHA-256 (`<hex>  {INSTALLER}` or a bare hex): {0:?}")]
    SidecarUnreadable(String),
    #[error(
        "{INSTALLER} does not match {SIDECAR}: the sidecar says {expected}, the download is {actual}; nothing was run"
    )]
    Mismatch { expected: String, actual: String },
}

/// The installer is offered on this system.
pub fn supported() -> Result<(), InstallError> {
    if cfg!(windows) {
        Err(InstallError::NoWindowsInstall)
    } else {
        Ok(())
    }
}

/// The hash a sidecar names: its first word, 64 hex digits, read in lower case.
pub fn expected_sha256(sidecar: &str) -> Result<String, InstallError> {
    let word = sidecar
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if word.len() == 64 && word.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(word)
    } else {
        Err(InstallError::SidecarUnreadable(
            sidecar.chars().take(200).collect(),
        ))
    }
}

/// An installer whose SHA-256 is the one its sidecar names. The bytes are private: the one
/// way to hold them is [`VerifiedInstaller::verify`], so nothing unverified reaches
/// [`VerifiedInstaller::run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedInstaller {
    /// the release tag the installer came from
    pub release: String,
    /// the SHA-256 the installer and its sidecar share, hex
    pub sha256: String,
    script: Vec<u8>,
}

impl VerifiedInstaller {
    pub fn verify(release: String, script: Vec<u8>, sidecar: &str) -> Result<Self, InstallError> {
        let expected = expected_sha256(sidecar)?;
        let actual = hex::encode(Sha256::digest(&script));
        if actual != expected {
            return Err(InstallError::Mismatch { expected, actual });
        }
        Ok(Self {
            release,
            sha256: actual,
            script,
        })
    }

    /// Run the script with `sh`, streaming both pipes into `out`; the exit status is the
    /// result, and `cancel` kills it ([`SatzError::Cancelled`]).
    ///
    /// The script is written into a private temporary directory (mode 0700 on unix, a name
    /// nobody can predict) and run from there, so no other user can swap it between the
    /// write and the run; the directory goes when the run returns. Its environment is the
    /// app's own with `SATZ_NO_MODIFY_PATH=1` added, and its stdin is closed.
    ///
    /// There is no such function on Windows, where satz has no build to install.
    #[cfg(not(windows))]
    pub async fn run(
        &self,
        out: mpsc::Sender<CliLine>,
        cancel: CancellationToken,
    ) -> Result<ExitStatus, SatzError> {
        let dir = tempfile::Builder::new()
            .prefix("satz-studio-install")
            .tempdir()
            .map_err(|e| SatzError::Io {
                context: "making a private directory for the satz installer".to_string(),
                source: e,
            })?;
        let path = dir.path().join(INSTALLER);
        std::fs::write(&path, &self.script).map_err(|e| SatzError::Io {
            context: format!("writing {}", path.display()),
            source: e,
        })?;
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg(&path)
            .current_dir(dir.path())
            .env(NO_MODIFY_PATH.0, NO_MODIFY_PATH.1)
            .stdin(Stdio::null())
            .kill_on_drop(true);
        let status = super::cli::stream(cmd, format!("sh {INSTALLER}"), out, cancel).await;
        drop(dir);
        status
    }
}

/// The installer of the latest satz release and its sidecar, downloaded and compared.
/// `api` is GitHub's API ([`github::API`]); a test gives a local server.
pub async fn fetch_verified(
    client: &reqwest::Client,
    api: &str,
) -> Result<VerifiedInstaller, InstallError> {
    supported()?;
    let release = github::latest_release(client, api, SATZ_REPO).await?;
    let installer = release
        .asset(INSTALLER)
        .ok_or_else(|| InstallError::NoInstaller {
            release: release.tag_name.clone(),
        })?;
    let sidecar = release
        .asset(SIDECAR)
        .ok_or_else(|| InstallError::NoSidecar {
            release: release.tag_name.clone(),
        })?;
    let script = github::download(client, &installer.browser_download_url).await?;
    let sidecar = github::download(client, &sidecar.browser_download_url).await?;
    VerifiedInstaller::verify(
        release.tag_name.clone(),
        script,
        &String::from_utf8_lossy(&sidecar),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &[u8] = b"#!/bin/sh\necho installed\n";

    fn sidecar_of(bytes: &[u8]) -> String {
        format!("{}  {INSTALLER}\n", hex::encode(Sha256::digest(bytes)))
    }

    #[test]
    fn the_sidecar_is_read_as_sha256sum_writes_it_or_as_a_bare_hash() {
        let hash = hex::encode(Sha256::digest(SCRIPT));
        assert_eq!(expected_sha256(&sidecar_of(SCRIPT)).unwrap(), hash);
        assert_eq!(expected_sha256(&hash.to_uppercase()).unwrap(), hash);
        for broken in ["", "not a hash", &hash[..63], &format!("{hash}0")] {
            assert!(
                matches!(
                    expected_sha256(broken),
                    Err(InstallError::SidecarUnreadable(_))
                ),
                "{broken:?}"
            );
        }
    }

    #[test]
    fn a_script_that_matches_its_sidecar_is_verified_and_one_that_does_not_is_refused() {
        let verified =
            VerifiedInstaller::verify("v0.59.7".to_string(), SCRIPT.to_vec(), &sidecar_of(SCRIPT))
                .unwrap();
        assert_eq!(verified.sha256, hex::encode(Sha256::digest(SCRIPT)));

        let other = b"#!/bin/sh\necho something else\n";
        let e =
            VerifiedInstaller::verify("v0.59.7".to_string(), other.to_vec(), &sidecar_of(SCRIPT))
                .unwrap_err();
        let said = e.to_string();
        assert!(matches!(e, InstallError::Mismatch { .. }), "{e:?}");
        assert!(said.contains("nothing was run"), "{said}");
    }

    #[test]
    fn windows_is_refused_by_name_and_everything_else_is_offered() {
        if cfg!(windows) {
            let said = supported().unwrap_err().to_string();
            assert!(said.contains("does not install satz on Windows"), "{said}");
            assert!(said.contains("satz-installer.ps1"), "{said}");
        } else {
            assert!(supported().is_ok());
        }
    }
}
