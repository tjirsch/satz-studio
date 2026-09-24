//! Starting the agentic client on one estate.
//!
//! satz-studio runs no model ([ADR 0020](../../../docs/adr/0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md)).
//! The configuration that points a client at the open estate is satz's own
//! ([`crate::satz::mcp_config`]); what is here is the other half — where the configured
//! client is, and running it in the estate's directory.

use std::io;
use std::path::{Path, PathBuf};

/// The agent command a fresh settings file starts with. `claude` is Claude Code; the
/// lookup is `PATH`, so the platform's own executable — `claude.cmd` on Windows — is
/// what is found.
pub const DEFAULT_AGENT_COMMAND: &str = "claude";

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("no agent command is configured — Settings names the client to start")]
    NoCommand,
    #[error("`{command}` is not on PATH: {source}")]
    NotFound {
        command: String,
        #[source]
        source: which::Error,
    },
}

/// Start the agent in `dir`.
///
/// The command line is the operator's own: its first word is the program, looked up on
/// `PATH` so a client that is not installed is named rather than failing silently, and
/// the rest are its arguments. It is then run through a one-shot script in the OS
/// terminal, the way `apply` and `bootstrap` are
/// ([ADR 0006](../../../docs/adr/0006-apply-and-bootstrap-run-in-the-users-terminal.md)):
/// the default client is a terminal program, and one started without a terminal shows
/// nothing. Nothing is supervised and nothing is read back — the agent's window is the
/// agent's.
///
/// Returns the script that was opened.
pub fn start(command: &str, dir: &Path) -> Result<PathBuf, AgentError> {
    locate(command)?;
    let script = script_path()?;
    write(&script, &script_text(command.trim(), dir))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).map_err(
            |source| AgentError::Io {
                path: script.clone(),
                source,
            },
        )?;
    }
    crate::satz::EstateSession::open_in_terminal(&script).map_err(|e| AgentError::Io {
        path: script.clone(),
        source: io::Error::other(e.to_string()),
    })?;
    Ok(script)
}

/// `cd <dir> && <command>` as a script for the platform's shell. The directory is quoted
/// so the shell reads it as written; the command line is the operator's own and is run
/// as they wrote it.
pub fn script_text(command: &str, dir: &Path) -> String {
    if cfg!(windows) {
        format!(
            "@echo off\r\ncd /d {} || exit /b 1\r\n{command}\r\n",
            crate::satz::session::cmd_path(dir)
        )
    } else {
        format!(
            "#!/bin/sh\ncd {} || exit 1\nexec {command}\n",
            crate::satz::session::sh_path(dir)
        )
    }
}

/// Where the configured client is: the command line's first word, looked up on `PATH`.
/// A line with nothing in it is [`AgentError::NoCommand`] and a program that is not
/// installed is [`AgentError::NotFound`] — the two the view tells apart.
pub fn locate(command: &str) -> Result<PathBuf, AgentError> {
    let program = program(command).ok_or(AgentError::NoCommand)?;
    which::which(&program).map_err(|source| AgentError::NotFound {
        command: program,
        source,
    })
}

/// The first word of a command line, with a double-quoted word kept whole; `None` for a
/// line with nothing in it.
pub fn program(command: &str) -> Option<String> {
    let command = command.trim();
    let mut out = String::new();
    let mut quoted = false;
    for c in command.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => break,
            c => out.push(c),
        }
    }
    (!out.is_empty()).then_some(out)
}

fn script_path() -> Result<PathBuf, AgentError> {
    let run_dir = crate::settings::data_dir()
        .map_err(|e| AgentError::Io {
            path: PathBuf::from("the app's data directory"),
            source: io::Error::other(e),
        })?
        .join("run");
    std::fs::create_dir_all(&run_dir).map_err(|source| AgentError::Io {
        path: run_dir.clone(),
        source,
    })?;
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AgentError::Io {
            path: run_dir.clone(),
            source: io::Error::other(e),
        })?
        .as_millis();
    Ok(run_dir.join(format!(
        "agent-{millis}.{}",
        if cfg!(windows) { "cmd" } else { "sh" }
    )))
}

fn write(path: &Path, text: &str) -> Result<(), AgentError> {
    std::fs::write(path, text).map_err(|source| AgentError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_word_is_the_program_and_an_empty_line_names_none() {
        assert_eq!(program("claude").as_deref(), Some("claude"));
        assert_eq!(program("  code --wait  ").as_deref(), Some("code"));
        assert_eq!(
            program("\"C:\\Program Files\\Agent\\agent.exe\" --here").as_deref(),
            Some("C:\\Program Files\\Agent\\agent.exe")
        );
        assert_eq!(program(""), None);
        assert_eq!(program("   "), None);
    }

    #[test]
    fn a_command_that_is_not_configured_and_one_that_is_not_installed_say_so() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            start("   ", dir.path()).unwrap_err(),
            AgentError::NoCommand
        ));
        let missing = start("satz-studio-no-such-agent", dir.path()).unwrap_err();
        assert!(matches!(missing, AgentError::NotFound { .. }));
        assert!(missing.to_string().contains("satz-studio-no-such-agent"));
    }

    #[test]
    fn the_script_changes_into_the_estate_directory_and_runs_the_command() {
        let text = script_text("claude", Path::new("/estates/acme"));
        assert!(text.contains("/estates/acme"));
        assert!(text.contains("claude"));
    }

    /// The directory reaches `cd` as written, whatever the shell would otherwise expand
    /// in it: the script is run, and the directory it lands in is the one named.
    #[cfg(unix)]
    #[test]
    fn a_directory_the_shell_would_expand_is_the_one_the_agent_starts_in() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("a $HOME `id` \\ \"q\" it's");
        std::fs::create_dir(&dir).unwrap();
        let script = root.path().join("agent.sh");
        std::fs::write(&script, script_text("pwd -P", &dir)).unwrap();
        let out = std::process::Command::new("sh")
            .arg(&script)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8(out.stdout).unwrap().trim_end(),
            dir.canonicalize().unwrap().display().to_string()
        );
    }
}
