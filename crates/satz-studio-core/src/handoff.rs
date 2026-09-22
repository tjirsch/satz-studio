//! Setting an agent up on one estate, and starting it.
//!
//! satz-studio runs no model ([ADR 0020](../../../docs/adr/0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md)).
//! What it does is point an agent that speaks the Model Context Protocol at the estate
//! on screen: the same `satz mcp --root <root> --allow <ceiling>` the app runs for
//! itself, rendered in the two shapes a client reads.
//!
//! - [`Handoff::project_file`] is `.mcp.json`, the project file Claude Code reads in the
//!   directory it is started in. Its server is named `satz`, so its tools carry the
//!   prefix `mcp__satz__`.
//! - [`Handoff::desktop_block`] is the `mcpServers` block for Claude Desktop's
//!   configuration file, which holds every server on the machine, so the server is named
//!   after the estate.
//!
//! Both are pure functions of the satz binary, the estate's session root, the estate's
//! name and the capability ceiling. The ceiling is written out because `satz mcp`
//! defaults to `--allow read`: a configuration that left it out would hand the agent a
//! server that cannot write and say nothing about it.

use std::io;
use std::path::{Path, PathBuf};

use crate::satz::Allow;

/// The file Claude Code reads in the directory it starts in.
pub const PROJECT_FILE: &str = ".mcp.json";

/// The server's name in the project file, and the prefix its tools carry
/// (`mcp__satz__…`).
pub const PROJECT_SERVER: &str = "satz";

/// The agent command a fresh settings file starts with. `claude` is Claude Code; the
/// lookup is `PATH`, so the platform's own executable — `claude.cmd` on Windows — is
/// what is found.
pub const DEFAULT_AGENT_COMMAND: &str = "claude";

/// What an agent needs to reach one estate: the satz to run, the boundary that satz
/// server is confined to, the estate's name and the ceiling it runs under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handoff {
    /// the satz binary the app located
    pub satz: PathBuf,
    /// the root `satz mcp` is confined to — [`crate::satz::session::session_root`] for
    /// this estate, not the estate directory
    pub root: PathBuf,
    /// the estate file's name, which names the server in Claude Desktop's file
    pub estate: String,
    pub allow: Allow,
}

impl Handoff {
    pub fn new(satz: &Path, root: &Path, estate: &str, allow: Allow) -> Self {
        Self {
            satz: satz.to_path_buf(),
            root: root.to_path_buf(),
            estate: estate.to_string(),
            allow,
        }
    }

    /// The arguments after the satz binary, as both shapes carry them.
    pub fn args(&self) -> Vec<String> {
        vec![
            "mcp".to_string(),
            "--root".to_string(),
            self.root.display().to_string(),
            "--allow".to_string(),
            self.allow.as_arg().to_string(),
        ]
    }

    /// The command line as a person reads it, for the view.
    pub fn command_line(&self) -> String {
        let mut line = self.satz.display().to_string();
        for arg in self.args() {
            line.push(' ');
            line.push_str(&arg);
        }
        line
    }

    /// The server's name in Claude Desktop's configuration file: one file there holds
    /// every server on the machine, so the estate is in the name. An estate whose name
    /// has nothing a server name can carry is just `satz`.
    pub fn server_name(&self) -> String {
        let slug = slug(&self.estate);
        if slug.is_empty() {
            PROJECT_SERVER.to_string()
        } else {
            format!("{PROJECT_SERVER}-{slug}")
        }
    }

    fn server(&self) -> serde_json::Value {
        serde_json::json!({
            "command": self.satz.display().to_string(),
            "args": self.args(),
        })
    }

    /// `.mcp.json`: the whole file, ending in a newline.
    pub fn project_file(&self) -> String {
        let doc = serde_json::json!({
            "mcpServers": { PROJECT_SERVER: self.server() },
        });
        format!("{}\n", pretty(&doc))
    }

    /// The block for Claude Desktop's configuration file, keyed by [`Self::server_name`]
    /// so several estates live beside each other.
    pub fn desktop_block(&self) -> String {
        let doc = serde_json::json!({
            "mcpServers": { self.server_name(): self.server() },
        });
        pretty(&doc)
    }

    /// Write [`Self::project_file`] as `<dir>/.mcp.json`.
    ///
    /// A file that is already those bytes is [`Written::Unchanged`] and nothing is
    /// written. A file that holds anything else is [`HandoffError::Exists`] — the
    /// operator may have written it by hand, or configured other servers in it — and
    /// only `overwrite` replaces it.
    pub fn write_project_file(&self, dir: &Path, overwrite: bool) -> Result<Written, HandoffError> {
        let path = dir.join(PROJECT_FILE);
        let text = self.project_file();
        match std::fs::read_to_string(&path) {
            Ok(existing) if existing == text => return Ok(Written::Unchanged(path)),
            Ok(_) if !overwrite => return Err(HandoffError::Exists { path }),
            Ok(_) => {
                write(&path, &text)?;
                return Ok(Written::Replaced(path));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(HandoffError::Io { path, source }),
        }
        write(&path, &text)?;
        Ok(Written::Created(path))
    }
}

/// What writing `.mcp.json` came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Written {
    Created(PathBuf),
    /// the file held other text and the operator asked for it to be replaced
    Replaced(PathBuf),
    /// the file already held exactly these bytes; nothing was written
    Unchanged(PathBuf),
}

impl Written {
    pub fn path(&self) -> &Path {
        match self {
            Written::Created(p) | Written::Replaced(p) | Written::Unchanged(p) => p,
        }
    }

    /// The sentence the toast carries.
    pub fn message(&self) -> String {
        match self {
            Written::Created(p) => format!("{} written", p.display()),
            Written::Replaced(p) => format!("{} replaced", p.display()),
            Written::Unchanged(p) => format!("{} already holds this configuration", p.display()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HandoffError {
    #[error(
        "{} holds a different configuration; replace it only if nothing else needs it",
        path.display()
    )]
    Exists { path: PathBuf },
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
    #[error("{}: a directory whose path holds a double quote cannot be started in", dir.display())]
    UnquotableDir { dir: PathBuf },
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
pub fn start(command: &str, dir: &Path) -> Result<PathBuf, HandoffError> {
    locate(command)?;
    if dir.display().to_string().contains('"') {
        return Err(HandoffError::UnquotableDir {
            dir: dir.to_path_buf(),
        });
    }
    let script = script_path()?;
    write(&script, &script_text(command.trim(), dir))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).map_err(
            |source| HandoffError::Io {
                path: script.clone(),
                source,
            },
        )?;
    }
    crate::satz::EstateSession::open_in_terminal(&script).map_err(|e| HandoffError::Io {
        path: script.clone(),
        source: io::Error::other(e.to_string()),
    })?;
    Ok(script)
}

/// `cd "<dir>" && <command>` as a script for the platform's shell.
pub fn script_text(command: &str, dir: &Path) -> String {
    if cfg!(windows) {
        format!(
            "@echo off\r\ncd /d \"{}\" || exit /b 1\r\n{command}\r\n",
            dir.display()
        )
    } else {
        format!(
            "#!/bin/sh\ncd \"{}\" || exit 1\nexec {command}\n",
            dir.display()
        )
    }
}

/// Where the configured client is: the command line's first word, looked up on `PATH`.
/// A line with nothing in it is [`HandoffError::NoCommand`] and a program that is not
/// installed is [`HandoffError::NotFound`] — the two the view tells apart.
pub fn locate(command: &str) -> Result<PathBuf, HandoffError> {
    let program = program(command).ok_or(HandoffError::NoCommand)?;
    which::which(&program).map_err(|source| HandoffError::NotFound {
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

fn script_path() -> Result<PathBuf, HandoffError> {
    let run_dir = crate::settings::data_dir()
        .map_err(|e| HandoffError::Io {
            path: PathBuf::from("the app's data directory"),
            source: io::Error::other(e),
        })?
        .join("run");
    std::fs::create_dir_all(&run_dir).map_err(|source| HandoffError::Io {
        path: run_dir.clone(),
        source,
    })?;
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| HandoffError::Io {
            path: run_dir.clone(),
            source: io::Error::other(e),
        })?
        .as_millis();
    Ok(run_dir.join(format!(
        "agent-{millis}.{}",
        if cfg!(windows) { "cmd" } else { "sh" }
    )))
}

fn write(path: &Path, text: &str) -> Result<(), HandoffError> {
    std::fs::write(path, text).map_err(|source| HandoffError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).expect("a JSON object of strings always serialises")
}

/// The estate's name as a server name can carry it: lower case, runs of anything else
/// folded into one `-`, the ends trimmed.
fn slug(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    let mut out = String::new();
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handoff() -> Handoff {
        Handoff::new(
            Path::new("/home/example/.local/bin/satz"),
            Path::new("/home/example/estates/acme"),
            "acme.satz",
            Allow::ReadWrite,
        )
    }

    fn servers(text: &str) -> serde_json::Map<String, serde_json::Value> {
        let doc: serde_json::Value = serde_json::from_str(text).expect("valid JSON");
        doc["mcpServers"]
            .as_object()
            .expect("an mcpServers object")
            .clone()
    }

    #[test]
    fn the_project_file_is_the_estates_own_satz_mcp_under_the_name_satz() {
        let h = handoff();
        let servers = servers(&h.project_file());
        assert_eq!(servers.len(), 1);
        let server = &servers[PROJECT_SERVER];
        assert_eq!(server["command"], "/home/example/.local/bin/satz");
        assert_eq!(
            server["args"],
            serde_json::json!([
                "mcp",
                "--root",
                "/home/example/estates/acme",
                "--allow",
                "read,write"
            ])
        );
    }

    #[test]
    fn the_ceiling_is_always_written_out_because_satz_defaults_to_read() {
        for allow in [Allow::Read, Allow::ReadWrite, Allow::ReadWriteExec] {
            let h = Handoff::new(
                Path::new("/usr/local/bin/satz"),
                Path::new("/estates/acme"),
                "acme.satz",
                allow,
            );
            for text in [h.project_file(), h.desktop_block()] {
                let servers = servers(&text);
                let args = servers.values().next().unwrap()["args"].clone();
                let args: Vec<String> = serde_json::from_value(args).unwrap();
                let at = args.iter().position(|a| a == "--allow").expect("--allow");
                assert_eq!(args[at + 1], allow.as_arg());
            }
        }
    }

    #[test]
    fn the_desktop_block_names_the_server_after_the_estate() {
        let h = handoff();
        assert_eq!(h.server_name(), "satz-acme");
        let servers = servers(&h.desktop_block());
        assert_eq!(servers.keys().collect::<Vec<_>>(), ["satz-acme"]);
        assert_eq!(
            servers["satz-acme"]["command"],
            "/home/example/.local/bin/satz"
        );
    }

    #[test]
    fn an_estate_name_becomes_a_server_name_or_the_bare_prefix() {
        let name = |estate: &str| {
            Handoff::new(
                Path::new("satz"),
                Path::new("/estates"),
                estate,
                Allow::ReadWrite,
            )
            .server_name()
        };
        assert_eq!(name("C0example.satz"), "satz-c0example");
        assert_eq!(name("acme prod.satz"), "satz-acme-prod");
        assert_eq!(name("__.satz"), "satz");
        assert_eq!(name(""), "satz");
    }

    #[test]
    fn a_path_with_a_space_survives_json_and_comes_back_whole() {
        let h = Handoff::new(
            Path::new("/Applications/satz tools/satz"),
            Path::new("/Users/example/My Estates/acme"),
            "acme.satz",
            Allow::ReadWriteExec,
        );
        let servers = servers(&h.project_file());
        let server = &servers[PROJECT_SERVER];
        assert_eq!(server["command"], "/Applications/satz tools/satz");
        assert_eq!(server["args"][2], "/Users/example/My Estates/acme");
    }

    #[test]
    fn the_command_line_is_the_two_flags_and_nothing_else() {
        assert_eq!(
            handoff().command_line(),
            "/home/example/.local/bin/satz mcp --root /home/example/estates/acme --allow read,write"
        );
    }

    #[test]
    fn the_written_file_round_trips_as_json() {
        let dir = tempfile::tempdir().unwrap();
        let h = handoff();
        let written = h.write_project_file(dir.path(), false).unwrap();
        assert_eq!(written, Written::Created(dir.path().join(PROJECT_FILE)));
        let text = std::fs::read_to_string(written.path()).unwrap();
        assert!(text.ends_with('\n'));
        assert_eq!(text, h.project_file());
        let back: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            back["mcpServers"][PROJECT_SERVER]["command"],
            "/home/example/.local/bin/satz"
        );
    }

    #[test]
    fn a_file_that_holds_these_bytes_is_left_alone_and_one_that_differs_is_not_clobbered() {
        let dir = tempfile::tempdir().unwrap();
        let h = handoff();
        h.write_project_file(dir.path(), false).unwrap();
        assert!(matches!(
            h.write_project_file(dir.path(), false).unwrap(),
            Written::Unchanged(_)
        ));

        let path = dir.path().join(PROJECT_FILE);
        let theirs = "{\n  \"mcpServers\": {\n    \"something-else\": {}\n  }\n}\n";
        std::fs::write(&path, theirs).unwrap();
        let refused = h.write_project_file(dir.path(), false).unwrap_err();
        assert!(matches!(refused, HandoffError::Exists { .. }));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), theirs);

        assert!(matches!(
            h.write_project_file(dir.path(), true).unwrap(),
            Written::Replaced(_)
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), h.project_file());
    }

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
            HandoffError::NoCommand
        ));
        let missing = start("satz-studio-no-such-agent", dir.path()).unwrap_err();
        assert!(matches!(missing, HandoffError::NotFound { .. }));
        assert!(missing.to_string().contains("satz-studio-no-such-agent"));
    }

    #[test]
    fn the_script_changes_into_the_estate_directory_and_runs_the_command() {
        let text = script_text("claude", Path::new("/estates/acme"));
        assert!(text.contains("/estates/acme"));
        assert!(text.contains("claude"));
    }
}
