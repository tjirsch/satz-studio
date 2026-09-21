//! satz's own installer, run for an operator who has no satz and, on Windows, for an
//! update: the cargo-dist installer of the latest satz release — the shell script on macOS
//! and Linux, the PowerShell script on Windows — verified against its SHA-256 sidecar
//! before a byte of it runs.
//!
//! - **One release object.** The installer and the sidecar are the assets of the release
//!   `releases/latest` names at that moment ([`crate::github::latest_release`]), never two
//!   separate `latest/download` URLs, which a release published in between would pair
//!   across two releases.
//! - **Verified before it runs.** A [`VerifiedInstaller`] exists only once the script's
//!   SHA-256 equals the sidecar's; a release without a sidecar, a sidecar that is not a
//!   SHA-256 and a mismatch are each a refusal, and nothing runs.
//! - **The folder is named, the `PATH` is left alone.** The installer is run with
//!   `SATZ_INSTALL_DIR` set to the folder the caller names — `~/.local/bin` for an install,
//!   the folder of the satz being replaced for an update — so no variable in the operator's
//!   environment sends it elsewhere, and with `SATZ_NO_MODIFY_PATH=1`, so it adds nothing
//!   to the shell profiles or the Windows user `PATH`: `SatzBinary::locate` searches
//!   `~/.local/bin` itself, and the running app keeps the `PATH` it was started with.
//! - **Nothing to answer.** The installer asks nothing; its stdin is closed all the same,
//!   so a prompt in some later version would read end-of-file rather than wait.

use std::ffi::OsString;
use std::path::Path;
use std::process::{ExitStatus, Stdio};

use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{CliLine, SatzError};
use crate::github::{self, GithubError};

/// satz's repository, whose latest release is installed.
pub const SATZ_REPO: &str = "tjirsch/satz";

/// The installer's switch for leaving the shell profiles and the Windows user `PATH` alone.
pub const NO_MODIFY_PATH: (&str, &str) = ("SATZ_NO_MODIFY_PATH", "1");

/// The installer's variable for the folder it writes satz into.
pub const INSTALL_DIR: &str = "SATZ_INSTALL_DIR";

/// Which of satz's two cargo-dist installers, and how it is run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installer {
    /// `satz-installer.sh`, run by `sh` — macOS and Linux
    Shell,
    /// `satz-installer.ps1`, run by `powershell -File` — Windows
    PowerShell,
}

impl Installer {
    /// The installer of the system this build runs on.
    pub const fn for_this_system() -> Self {
        if cfg!(windows) {
            Installer::PowerShell
        } else {
            Installer::Shell
        }
    }

    /// The release asset.
    pub const fn asset(self) -> &'static str {
        match self {
            Installer::Shell => "satz-installer.sh",
            Installer::PowerShell => "satz-installer.ps1",
        }
    }

    /// The checksum satz's `attach-checksum` job attaches to every release: `sha256sum`'s
    /// output, `<hex>  <asset>`.
    pub const fn sidecar(self) -> &'static str {
        match self {
            Installer::Shell => "satz-installer.sh.sha256",
            Installer::PowerShell => "satz-installer.ps1.sha256",
        }
    }

    /// The program that runs the script.
    pub const fn program(self) -> &'static str {
        match self {
            Installer::Shell => "sh",
            Installer::PowerShell => "powershell",
        }
    }

    /// The arguments that run `script`. PowerShell runs it with no profile, no prompt, and
    /// past the execution policy, which refuses an unsigned script file by default; `-File`
    /// is the last switch, because everything after the path is the script's own.
    pub fn args(self, script: &Path) -> Vec<OsString> {
        match self {
            Installer::Shell => vec![script.as_os_str().to_owned()],
            Installer::PowerShell => [
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ]
            .into_iter()
            .map(OsString::from)
            .chain([script.as_os_str().to_owned()])
            .collect(),
        }
    }

    /// The run as one line for a log header: the environment, the program, the arguments,
    /// with the script named by its asset.
    pub fn command_line(self, install_dir: &Path) -> String {
        let args = self
            .args(Path::new(self.asset()))
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "{INSTALL_DIR}={} {}={} {} {args}",
            install_dir.display(),
            NO_MODIFY_PATH.0,
            NO_MODIFY_PATH.1,
            self.program()
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error(transparent)]
    Github(#[from] GithubError),
    #[error("the satz release {release} has no {asset} — its release build has not finished")]
    NoInstaller {
        release: String,
        asset: &'static str,
    },
    #[error(
        "the satz release {release} has no {sidecar}, so its installer cannot be verified and is not run"
    )]
    NoSidecar {
        release: String,
        sidecar: &'static str,
    },
    #[error("{sidecar} is not a SHA-256 (`<hex>  {asset}` or a bare hex): {text:?}")]
    SidecarUnreadable {
        sidecar: &'static str,
        asset: &'static str,
        text: String,
    },
    #[error(
        "{asset} does not match {sidecar}: the sidecar says {expected}, the download is {actual}; nothing was run"
    )]
    Mismatch {
        asset: &'static str,
        sidecar: &'static str,
        expected: String,
        actual: String,
    },
}

/// The hash a sidecar names: its first word, 64 hex digits, read in lower case; `None` for
/// anything else.
pub fn expected_sha256(sidecar: &str) -> Option<String> {
    let word = sidecar
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    (word.len() == 64 && word.bytes().all(|b| b.is_ascii_hexdigit())).then_some(word)
}

/// An installer whose SHA-256 is the one its sidecar names. The bytes are private: the one
/// way to hold them is [`VerifiedInstaller::verify`], so nothing unverified reaches
/// [`VerifiedInstaller::run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedInstaller {
    /// the release tag the installer came from
    pub release: String,
    /// which installer it is
    pub installer: Installer,
    /// the SHA-256 the installer and its sidecar share, hex
    pub sha256: String,
    script: Vec<u8>,
}

impl VerifiedInstaller {
    pub fn verify(
        installer: Installer,
        release: String,
        script: Vec<u8>,
        sidecar: &str,
    ) -> Result<Self, InstallError> {
        let expected = expected_sha256(sidecar).ok_or_else(|| InstallError::SidecarUnreadable {
            sidecar: installer.sidecar(),
            asset: installer.asset(),
            text: sidecar.chars().take(200).collect(),
        })?;
        let actual = hex::encode(Sha256::digest(&script));
        if actual != expected {
            return Err(InstallError::Mismatch {
                asset: installer.asset(),
                sidecar: installer.sidecar(),
                expected,
                actual,
            });
        }
        Ok(Self {
            release,
            installer,
            sha256: actual,
            script,
        })
    }

    /// Run the script, streaming both pipes into `out`; the exit status is the result, and
    /// `cancel` kills it ([`SatzError::Cancelled`]).
    ///
    /// The script is written, under its asset name, into a private temporary directory
    /// (mode 0700 on unix, a name nobody can predict) and run from there, so no other user
    /// can swap it between the write and the run; the directory goes when the run returns.
    /// Its environment is the app's own with `SATZ_INSTALL_DIR=<install_dir>` and
    /// `SATZ_NO_MODIFY_PATH=1` added, and its stdin is closed.
    pub async fn run(
        &self,
        install_dir: &Path,
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
        let path = dir.path().join(self.installer.asset());
        std::fs::write(&path, &self.script).map_err(|e| SatzError::Io {
            context: format!("writing {}", path.display()),
            source: e,
        })?;
        let mut cmd = tokio::process::Command::new(self.installer.program());
        cmd.args(self.installer.args(&path))
            .current_dir(dir.path())
            .env(INSTALL_DIR, install_dir)
            .env(NO_MODIFY_PATH.0, NO_MODIFY_PATH.1)
            .stdin(Stdio::null())
            .kill_on_drop(true);
        let status = super::cli::stream(
            cmd,
            format!("{} {}", self.installer.program(), self.installer.asset()),
            out,
            cancel,
        )
        .await;
        drop(dir);
        status
    }
}

/// `installer` of the latest satz release and its sidecar, downloaded and compared. The
/// app passes [`Installer::for_this_system`]; a test names each. `api` is GitHub's API
/// ([`github::API`]); a test gives a local server.
pub async fn fetch_verified(
    installer: Installer,
    client: &reqwest::Client,
    api: &str,
) -> Result<VerifiedInstaller, InstallError> {
    let release = github::latest_release(client, api, SATZ_REPO).await?;
    let script_asset =
        release
            .asset(installer.asset())
            .ok_or_else(|| InstallError::NoInstaller {
                release: release.tag_name.clone(),
                asset: installer.asset(),
            })?;
    let sidecar_asset =
        release
            .asset(installer.sidecar())
            .ok_or_else(|| InstallError::NoSidecar {
                release: release.tag_name.clone(),
                sidecar: installer.sidecar(),
            })?;
    let script = github::download(client, &script_asset.browser_download_url).await?;
    let sidecar = github::download(client, &sidecar_asset.browser_download_url).await?;
    VerifiedInstaller::verify(
        installer,
        release.tag_name.clone(),
        script,
        &String::from_utf8_lossy(&sidecar),
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    const SCRIPT: &[u8] = b"#!/bin/sh\necho installed\n";

    fn sidecar_of(installer: Installer, bytes: &[u8]) -> String {
        format!(
            "{}  {}\n",
            hex::encode(Sha256::digest(bytes)),
            installer.asset()
        )
    }

    #[test]
    fn the_sidecar_is_read_as_sha256sum_writes_it_or_as_a_bare_hash() {
        let hash = hex::encode(Sha256::digest(SCRIPT));
        for installer in [Installer::Shell, Installer::PowerShell] {
            assert_eq!(
                expected_sha256(&sidecar_of(installer, SCRIPT)).as_deref(),
                Some(hash.as_str())
            );
        }
        assert_eq!(
            expected_sha256(&hash.to_uppercase()).as_deref(),
            Some(hash.as_str())
        );
        // a sidecar written on Windows ends its line with CRLF
        assert_eq!(
            expected_sha256(&format!("{hash}  satz-installer.ps1\r\n")).as_deref(),
            Some(hash.as_str())
        );
        for broken in ["", "not a hash", &hash[..63], &format!("{hash}0")] {
            assert_eq!(expected_sha256(broken), None, "{broken:?}");
        }
    }

    #[test]
    fn a_script_that_matches_its_sidecar_is_verified_and_one_that_does_not_is_refused() {
        for installer in [Installer::Shell, Installer::PowerShell] {
            let verified = VerifiedInstaller::verify(
                installer,
                "v0.59.7".to_string(),
                SCRIPT.to_vec(),
                &sidecar_of(installer, SCRIPT),
            )
            .unwrap();
            assert_eq!(verified.sha256, hex::encode(Sha256::digest(SCRIPT)));
            assert_eq!(verified.installer, installer);

            let other = b"#!/bin/sh\necho something else\n";
            let e = VerifiedInstaller::verify(
                installer,
                "v0.59.7".to_string(),
                other.to_vec(),
                &sidecar_of(installer, SCRIPT),
            )
            .unwrap_err();
            let said = e.to_string();
            assert!(matches!(e, InstallError::Mismatch { .. }), "{e:?}");
            assert!(said.contains("nothing was run"), "{said}");
            assert!(said.contains(installer.asset()), "{said}");

            let e = VerifiedInstaller::verify(
                installer,
                "v0.59.7".to_string(),
                SCRIPT.to_vec(),
                "<html>",
            )
            .unwrap_err();
            let said = e.to_string();
            assert!(matches!(e, InstallError::SidecarUnreadable { .. }), "{e:?}");
            assert!(said.contains(installer.sidecar()), "{said}");
        }
    }

    #[test]
    fn each_system_fetches_its_own_installer_and_sidecar() {
        assert_eq!(Installer::Shell.asset(), "satz-installer.sh");
        assert_eq!(Installer::Shell.sidecar(), "satz-installer.sh.sha256");
        assert_eq!(Installer::PowerShell.asset(), "satz-installer.ps1");
        assert_eq!(Installer::PowerShell.sidecar(), "satz-installer.ps1.sha256");
        assert_eq!(
            Installer::for_this_system(),
            if cfg!(windows) {
                Installer::PowerShell
            } else {
                Installer::Shell
            }
        );
    }

    #[test]
    fn the_shell_script_runs_under_sh_and_the_powershell_one_past_the_execution_policy() {
        let script = PathBuf::from("dir").join("x");
        assert_eq!(Installer::Shell.program(), "sh");
        assert_eq!(
            Installer::Shell.args(&script),
            [script.as_os_str().to_owned()]
        );
        assert_eq!(Installer::PowerShell.program(), "powershell");
        assert_eq!(
            Installer::PowerShell.args(&script),
            [
                OsString::from("-NoProfile"),
                OsString::from("-NonInteractive"),
                OsString::from("-ExecutionPolicy"),
                OsString::from("Bypass"),
                OsString::from("-File"),
                script.as_os_str().to_owned(),
            ]
        );
        let bin = PathBuf::from("bin");
        assert_eq!(
            Installer::PowerShell.command_line(&bin),
            "SATZ_INSTALL_DIR=bin SATZ_NO_MODIFY_PATH=1 powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File satz-installer.ps1"
        );
        assert_eq!(
            Installer::Shell.command_line(&bin),
            "SATZ_INSTALL_DIR=bin SATZ_NO_MODIFY_PATH=1 sh satz-installer.sh"
        );
    }
}
