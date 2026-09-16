//! The Claude Code CLI as the app sees it: where the binary is, which version it is,
//! whether it is signed in, and the two commands that sign it in and out — each a
//! one-shot script the user's own terminal runs, because a browser login needs a
//! terminal and not a hidden child process.
//!
//! satz-studio never reads Claude Code's credential. `claude auth status` is the whole
//! of what the app knows about the account, and the e-mail it carries is shown in
//! Settings and nowhere else: [`AuthStatus`]'s `Debug` redacts it, so a `tracing` line
//! or a panic message cannot carry it.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::llm::ClaudeError;

#[derive(Debug, thiserror::Error)]
pub enum ClaudeCodeError {
    #[error("the Claude Code CLI was not found; tried {}", tried.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "))]
    NotFound { tried: Vec<PathBuf> },
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("`claude {command}` exited with {status}:\n{stderr}")]
    Exit {
        command: String,
        status: String,
        stderr: String,
    },
    #[error("`claude {command}` printed JSON this app does not read: {source}")]
    Json {
        command: String,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "{} is a batch file, and a turn cannot be handed to one safely: Claude Code is given a JSON MCP configuration and a multi-line system prompt, and neither can be quoted for cmd.exe. Rust refuses to try (CVE-2024-24576) and working around it would re-open that hole, so satz-studio does not. Point Settings at Claude Code's own executable instead of the shim a package manager put on PATH.",
        path.display()
    )]
    BatchFile { path: PathBuf },
    #[error("claude code: {0}")]
    Protocol(String),
    #[error("Claude Code is signed out — sign in from Settings, or run `claude auth login`")]
    NotSignedIn,
    #[error("the turn failed: {0}")]
    Turn(String),
    #[error("cancelled")]
    Cancelled,
}

impl From<ClaudeCodeError> for ClaudeError {
    fn from(e: ClaudeCodeError) -> Self {
        ClaudeError::from(&e)
    }
}

/// The one conversion into the app's error: the message as the variant prints it, once.
/// A failure is carried as it is and never wrapped in another variant first — each
/// variant prints its own prefix, and a wrapped one prints it twice.
impl From<&ClaudeCodeError> for ClaudeError {
    fn from(e: &ClaudeCodeError) -> Self {
        match e {
            ClaudeCodeError::Cancelled => ClaudeError::Cancelled,
            other => ClaudeError::ClaudeCode(other.to_string()),
        }
    }
}

/// What `claude auth status --json` answered.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    #[serde(default)]
    pub logged_in: bool,
    /// `claude.ai` for a subscription login, `console` for an API key
    #[serde(default)]
    pub auth_method: Option<String>,
    #[serde(default)]
    pub api_provider: Option<String>,
    /// the account's e-mail. Shown in Settings; never logged, never persisted, never
    /// in a fixture.
    #[serde(default)]
    pub email: Option<String>,
}

/// The e-mail is redacted: `Debug` reaches log lines, panic messages and test output,
/// and none of them is a place for the account's address.
impl std::fmt::Debug for AuthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthStatus")
            .field("logged_in", &self.logged_in)
            .field("auth_method", &self.auth_method)
            .field("api_provider", &self.api_provider)
            .field(
                "email",
                &self.email.as_ref().map(|_| "<redacted>").unwrap_or("none"),
            )
            .finish()
    }
}

impl AuthStatus {
    /// Signed in through a claude.ai account rather than an API key — the login this
    /// backend exists for.
    pub fn is_subscription(&self) -> bool {
        self.logged_in && self.auth_method.as_deref() == Some("claude.ai")
    }
}

/// Whether a path is a Windows batch file. `--version` would run through one, so the
/// refusal cannot wait for a spawn that fails: it is the TURN that cannot be passed, and
/// by then the operator is looking at a chat window. Judged by extension on every
/// platform so the rule is testable off Windows too.
fn is_batch_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("bat") || e.eq_ignore_ascii_case("cmd"))
}

/// The located Claude Code binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeCodeCli {
    pub path: PathBuf,
    /// what `claude --version` printed, e.g. `2.1.270`
    pub version: String,
}

impl ClaudeCodeCli {
    /// The Settings override, then `claude` on `PATH`, then `~/.local/bin/claude`; the
    /// first that exists is run with `--version`. An override that does not exist is
    /// [`ClaudeCodeError::NotFound`] naming it, and the search never continues past it.
    pub async fn locate(override_path: Option<&Path>) -> Result<Self, ClaudeCodeError> {
        let path = match override_path {
            Some(p) if p.is_file() => p.to_path_buf(),
            Some(p) => {
                return Err(ClaudeCodeError::NotFound {
                    tried: vec![p.to_path_buf()],
                });
            }
            None => Self::first_on_path_or_home()?,
        };
        if is_batch_file(&path) {
            return Err(ClaudeCodeError::BatchFile { path });
        }
        let output = run(&path, &["--version"]).await?;
        let version = parse_version(&output).ok_or_else(|| {
            ClaudeCodeError::Protocol(format!(
                "`{} --version` printed no version this app reads: {output:?}",
                path.display()
            ))
        })?;
        Ok(ClaudeCodeCli { path, version })
    }

    fn first_on_path_or_home() -> Result<PathBuf, ClaudeCodeError> {
        let mut tried = Vec::new();
        match which::which("claude") {
            Ok(p) => return Ok(p),
            Err(_) => tried.push(PathBuf::from("claude (on PATH)")),
        }
        if let Some(home) = dirs::home_dir() {
            let local = home.join(".local").join("bin").join("claude");
            if local.is_file() {
                return Ok(local);
            }
            tried.push(local);
        }
        Err(ClaudeCodeError::NotFound { tried })
    }

    /// `claude auth status --json`.
    pub async fn auth_status(&self) -> Result<AuthStatus, ClaudeCodeError> {
        let output = run(&self.path, &["auth", "status", "--json"]).await?;
        serde_json::from_str(&output).map_err(|e| ClaudeCodeError::Json {
            command: "auth status".to_string(),
            source: e,
        })
    }

    /// The shell line that signs the account in. It runs in the user's own terminal
    /// (the login opens a browser and waits), never as a child of the app.
    pub fn login_command(&self) -> String {
        format!("{} auth login", quote(&self.path))
    }

    /// The shell line that signs the account out.
    pub fn logout_command(&self) -> String {
        format!("{} auth logout", quote(&self.path))
    }

    /// Write `line` as a one-shot script under `<data dir>/satz-studio/run/` and open
    /// it with the OS terminal (the same opener `apply` and `bootstrap` use). Returns
    /// the script's path.
    pub fn open_in_terminal(line: &str) -> Result<PathBuf, ClaudeCodeError> {
        let run_dir = crate::settings::data_dir()
            .map_err(|e| ClaudeCodeError::Io {
                context: "the app's data directory".to_string(),
                source: std::io::Error::other(e),
            })?
            .join("run");
        std::fs::create_dir_all(&run_dir).map_err(|e| ClaudeCodeError::Io {
            context: format!("creating {}", run_dir.display()),
            source: e,
        })?;
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| ClaudeCodeError::Io {
                context: "the system clock".to_string(),
                source: std::io::Error::other(e),
            })?
            .as_millis();
        let script = run_dir.join(format!(
            "claude-{millis}.{}",
            if cfg!(windows) { "cmd" } else { "sh" }
        ));
        let text = if cfg!(windows) {
            format!("@echo off\r\n{line}\r\n")
        } else {
            format!("#!/bin/sh\n{line}\n")
        };
        std::fs::write(&script, text).map_err(|e| ClaudeCodeError::Io {
            context: format!("writing {}", script.display()),
            source: e,
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).map_err(
                |e| ClaudeCodeError::Io {
                    context: format!("making {} executable", script.display()),
                    source: e,
                },
            )?;
        }
        crate::satz::EstateSession::open_in_terminal(&script).map_err(|e| ClaudeCodeError::Io {
            context: format!("opening {} in a terminal", script.display()),
            source: std::io::Error::other(e.to_string()),
        })?;
        Ok(script)
    }
}

/// One short `claude` invocation, its stdout on success.
async fn run(path: &Path, args: &[&str]) -> Result<String, ClaudeCodeError> {
    let output = tokio::process::Command::new(path)
        .args(args)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| ClaudeCodeError::Io {
            context: format!("running `{} {}`", path.display(), args.join(" ")),
            source: e,
        })?;
    if !output.status.success() {
        return Err(ClaudeCodeError::Exit {
            command: args.join(" "),
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The version in `claude --version` output (`2.1.270 (Claude Code)`).
pub fn parse_version(output: &str) -> Option<String> {
    let word = output.split_whitespace().next()?;
    let word = word.trim_start_matches('v');
    let mut parts = word.split('.');
    let numeric =
        |p: Option<&str>| p.is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    (numeric(parts.next()) && numeric(parts.next())).then(|| word.to_string())
}

/// A path as one shell word.
fn quote(path: &Path) -> String {
    let text = path.display().to_string();
    if cfg!(windows) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_the_first_word_of_the_line() {
        assert_eq!(
            parse_version("2.1.270 (Claude Code)").as_deref(),
            Some("2.1.270")
        );
        assert_eq!(parse_version("v2.1.270").as_deref(), Some("2.1.270"));
        assert_eq!(parse_version("Claude Code"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn debug_never_prints_the_account_address() {
        let status = AuthStatus {
            logged_in: true,
            auth_method: Some("claude.ai".to_string()),
            api_provider: Some("firstParty".to_string()),
            email: Some("first.admin@example.com".to_string()),
        };
        let printed = format!("{status:?}");
        assert!(!printed.contains("first.admin"), "{printed}");
        assert!(printed.contains("<redacted>"), "{printed}");
        assert!(status.is_subscription());
    }

    #[test]
    fn the_login_and_logout_lines_quote_the_binary() {
        let cli = ClaudeCodeCli {
            path: PathBuf::from("/opt/a b/claude"),
            version: "2.1.270".to_string(),
        };
        assert_eq!(cli.login_command(), "\"/opt/a b/claude\" auth login");
        assert_eq!(cli.logout_command(), "\"/opt/a b/claude\" auth logout");
    }
}
