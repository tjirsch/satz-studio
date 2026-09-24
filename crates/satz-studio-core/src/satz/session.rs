//! One session per open estate: the CLI runner and the MCP child, the write lock every
//! writer takes, and the identity the estate's live tools run as — read from
//! `satz_open`, displayed, configured nowhere. Every tool call the window makes goes
//! through [`EstateSession::tool`], so one identity per estate holds.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::{Mutex, MutexGuard};

use super::mcp_config::{self, Client, Run};
use super::{Allow, McpSession, SatzBinary, SatzCli, SatzError, ToolOutcome};
use crate::estate::EstateDir;

pub struct EstateSession {
    pub dir: EstateDir,
    /// the main `.satz` file, absolute
    pub main: PathBuf,
    pub cli: SatzCli,
    /// the boundary `satz mcp` is confined to: the `--root` `satz mcp-config` renders for
    /// this estate, the same one every client's configuration carries
    pub root: PathBuf,
    mcp: McpSession,
    write_lock: Mutex<()>,
}

impl std::fmt::Debug for EstateSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EstateSession")
            .field("main", &self.main)
            .field("runs_as", &self.runs_as())
            .finish()
    }
}

impl EstateSession {
    /// Open the estate: `main` resolves as satz resolves a name on the command line and
    /// is made absolute; `satz mcp` is rooted where `satz mcp-config` says a client's
    /// server is rooted — one rule for the root, satz's — and started at
    /// [`Allow::STUDIO`], the ceiling the window's own writes need; `satz_open` gets the
    /// absolute `config.toml` and the absolute main file.
    pub async fn open(
        bin: &SatzBinary,
        dir: EstateDir,
        main: PathBuf,
    ) -> Result<Arc<EstateSession>, SatzError> {
        let dir = if dir.config_path.is_absolute() {
            dir
        } else {
            let config = absolute(&dir.config_path)?;
            EstateDir::open(&config).map_err(|e| SatzError::Io {
                context: format!("reopening {}", config.display()),
                source: std::io::Error::other(e),
            })?
        };
        let main = absolute(&dir.estate_path(&main))?;
        let cli = SatzCli::new(bin.clone(), dir.dir.clone());
        let printed = mcp_config::run(
            &cli,
            utf8(&main)?,
            Client::ClaudeCode,
            Allow::STUDIO,
            Run::Show,
        )
        .await?;
        let root = mcp_config::root(&printed)?;
        let mcp = McpSession::open(
            bin,
            &root,
            Allow::STUDIO,
            utf8(&dir.config_path)?,
            utf8(&main)?,
        )
        .await?;
        Ok(Arc::new(EstateSession {
            dir,
            main,
            cli,
            root,
            mcp,
            write_lock: Mutex::new(()),
        }))
    }

    /// The identity this estate's live tools run as, as `satz_open` reported it.
    pub fn runs_as(&self) -> Option<&str> {
        self.mcp.open_report().runs_as.as_deref()
    }
    pub fn deployment_mode(&self) -> Option<&str> {
        self.mcp.open_report().deployment_mode.as_deref()
    }
    /// Call a tool on this estate's session.
    pub async fn tool(
        &self,
        name: &str,
        args: serde_json::Map<String, serde_json::Value>,
    ) -> Result<ToolOutcome, SatzError> {
        self.mcp.call(name, args).await
    }

    /// Every writer in the window takes this first: a view edit, an interview answer, a
    /// pack switched. Held across the check and the rename.
    pub async fn write_lock(&self) -> MutexGuard<'_, ()> {
        self.write_lock.lock().await
    }

    /// `apply` and `bootstrap` run in the user's own terminal: a one-shot script under
    /// the app's data directory (`<data dir>/run/<unix millis>.sh`, `.cmd` on Windows)
    /// holding `cd '<estate>' && '<satz>' --config . <args…>`, opened with the OS
    /// terminal. Returns the script's path.
    pub fn external_command(&self, args: &[String]) -> Result<PathBuf, SatzError> {
        let run_dir = crate::settings::data_dir()
            .map_err(|e| SatzError::Io {
                context: "the app's data directory".to_string(),
                source: std::io::Error::other(e),
            })?
            .join("run");
        std::fs::create_dir_all(&run_dir).map_err(|e| SatzError::Io {
            context: format!("creating {}", run_dir.display()),
            source: e,
        })?;
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| SatzError::Io {
                context: "the system clock".to_string(),
                source: std::io::Error::other(e),
            })?
            .as_millis();
        let script = run_dir.join(format!(
            "{millis}.{}",
            if cfg!(windows) { "cmd" } else { "sh" }
        ));
        let text = script_text(&self.dir.dir, &self.cli.bin.path, args);
        std::fs::write(&script, text).map_err(|e| SatzError::Io {
            context: format!("writing {}", script.display()),
            source: e,
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).map_err(
                |e| SatzError::Io {
                    context: format!("making {} executable", script.display()),
                    source: e,
                },
            )?;
        }
        Ok(script)
    }

    /// Open a script in the OS terminal: macOS `open -a Terminal`, Linux the first of
    /// `x-terminal-emulator`, `gnome-terminal`, `konsole`, `xterm` on `PATH` running
    /// `bash <script>`, Windows `cmd /c start "satz" cmd /k <script>`.
    pub fn open_in_terminal(script: &Path) -> Result<(), SatzError> {
        open_in_terminal(script)
    }
}

fn absolute(p: &Path) -> Result<PathBuf, SatzError> {
    std::path::absolute(p).map_err(|e| SatzError::Io {
        context: format!("resolving {}", p.display()),
        source: e,
    })
}

fn utf8(p: &Path) -> Result<&str, SatzError> {
    p.to_str().ok_or_else(|| {
        SatzError::Mcp(format!(
            "{}: not a UTF-8 path, and satz_open takes a string",
            p.display()
        ))
    })
}

/// `cd <dir> && <satz> --config . <args…>` as a script for the platform's shell, the two
/// paths quoted so the shell reads them as written.
fn script_text(dir: &Path, satz: &Path, args: &[String]) -> String {
    if cfg!(windows) {
        let args = args
            .iter()
            .map(|a| cmd_quote(a))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "@echo off\r\ncd /d {} && {} --config . {args}\r\n",
            cmd_path(dir),
            cmd_path(satz)
        )
    } else {
        let args = args
            .iter()
            .map(|a| sh_quote(a))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "#!/bin/sh\nset -e\ncd {} && {} --config . {args}\n",
            sh_path(dir),
            sh_path(satz)
        )
    }
}

/// A path for `sh`, single-quoted: nothing inside single quotes is expanded — not `$`,
/// not a backtick, not a backslash — and a single quote in the path is closed, escaped
/// and reopened (`'\''`).
pub(crate) fn sh_path(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

/// A path for `cmd.exe` in a script: double-quoted, and `%` doubled, which is how a
/// batch file reads a literal percent sign rather than a variable. A Windows path holds
/// no double quote.
pub(crate) fn cmd_path(path: &Path) -> String {
    format!("\"{}\"", path.display().to_string().replace('%', "%%"))
}

/// A word that needs no quoting is left bare; anything else is double-quoted with
/// `"`, `\`, `$` and `` ` `` escaped.
fn sh_quote(arg: &str) -> String {
    if is_bare(arg) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    for c in arg.chars() {
        if "\"\\$`".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// `cmd.exe` quoting: double quotes around the word, an inner quote doubled.
fn cmd_quote(arg: &str) -> String {
    if is_bare(arg) {
        arg.to_string()
    } else {
        format!("\"{}\"", arg.replace('"', "\"\""))
    }
}

/// A word every shell passes through unchanged.
fn is_bare(arg: &str) -> bool {
    !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./=:@%+,".contains(c))
}

#[cfg(target_os = "macos")]
fn open_in_terminal(script: &Path) -> Result<(), SatzError> {
    let status = std::process::Command::new("open")
        .arg("-a")
        .arg("Terminal")
        .arg(script)
        .status()
        .map_err(|e| SatzError::Io {
            context: "running `open -a Terminal`".to_string(),
            source: e,
        })?;
    if !status.success() {
        return Err(SatzError::Exit {
            command: format!("open -a Terminal {}", script.display()),
            status,
            stderr: String::new(),
        });
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn open_in_terminal(script: &Path) -> Result<(), SatzError> {
    const TERMINALS: [(&str, &[&str]); 4] = [
        ("x-terminal-emulator", &["-e"]),
        ("gnome-terminal", &["--"]),
        ("konsole", &["-e"]),
        ("xterm", &["-e"]),
    ];
    let Some((name, flags, path)) = TERMINALS
        .iter()
        .find_map(|(name, flags)| which::which(name).ok().map(|p| (*name, *flags, p)))
    else {
        let names = TERMINALS
            .iter()
            .map(|(n, _)| *n)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(SatzError::NotFound {
            tried: vec![PathBuf::from(format!("a terminal on PATH: {names}"))],
        });
    };
    let mut child = std::process::Command::new(path)
        .args(flags)
        .arg("bash")
        .arg(script)
        .spawn()
        .map_err(|e| SatzError::Io {
            context: format!("starting {name}"),
            source: e,
        })?;
    // The terminal lives as long as its window; it is reaped when it closes, and how
    // the window closed says nothing about the command it ran.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_in_terminal(script: &Path) -> Result<(), SatzError> {
    let status = std::process::Command::new("cmd")
        .args(["/c", "start", "satz", "cmd", "/k"])
        .arg(script)
        .status()
        .map_err(|e| SatzError::Io {
            context: "running `cmd /c start`".to_string(),
            source: e,
        })?;
    if !status.success() {
        return Err(SatzError::Exit {
            command: format!("cmd /c start satz cmd /k {}", script.display()),
            status,
            stderr: String::new(),
        });
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn open_in_terminal(script: &Path) -> Result<(), SatzError> {
    Err(SatzError::NotFound {
        tried: vec![PathBuf::from(format!(
            "a terminal for {} on this platform",
            script.display()
        ))],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_changes_into_the_estate_and_quotes_what_needs_it() {
        let text = script_text(
            Path::new("/estates/acme"),
            Path::new("/usr/local/bin/satz"),
            &["apply".to_string(), "a b".to_string(), "x\"y".to_string()],
        );
        if cfg!(windows) {
            assert!(text.contains("cd /d \"/estates/acme\" && \"/usr/local/bin/satz\" --config . apply \"a b\" \"x\"\"y\""), "{text}");
        } else {
            assert!(text.starts_with("#!/bin/sh\nset -e\n"), "{text}");
            assert!(text.contains("cd '/estates/acme' && '/usr/local/bin/satz' --config . apply \"a b\" \"x\\\"y\"\n"), "{text}");
        }
    }

    /// A directory the shell would otherwise read — a `$`, a backtick, a backslash, a
    /// quote of either kind — reaches `cd` as written.
    #[cfg(unix)]
    #[test]
    fn a_path_the_shell_would_expand_is_passed_as_written() {
        let dir = Path::new("/estates/$HOME `id` \\ \"q\" it's");
        let quoted = sh_path(dir);
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf '%s' {quoted}"))
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            dir.display().to_string()
        );
    }

    #[test]
    fn a_percent_sign_is_doubled_for_a_batch_file() {
        assert_eq!(cmd_path(Path::new("C:\\a%PATH%b")), "\"C:\\a%%PATH%%b\"");
    }
}
