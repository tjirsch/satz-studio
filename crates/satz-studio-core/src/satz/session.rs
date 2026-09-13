//! One session per open estate: the CLI runner and the MCP child, the write lock every
//! writer takes, and the identity the estate's live tools run as — read from
//! `satz_open`, displayed, configured nowhere. The Commands view and the agent's tool
//! bridge both go through [`EstateSession::tool`], so one identity per estate holds.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use tokio::sync::{Mutex, MutexGuard};

use super::{Allow, McpSession, SatzBinary, SatzCli, SatzError, ToolInfo, ToolOutcome};
use crate::estate::EstateDir;

pub struct EstateSession {
    pub dir: EstateDir,
    /// the main `.satz` file, absolute
    pub main: PathBuf,
    pub cli: SatzCli,
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
    /// is made absolute; `satz mcp` is rooted at [`session_root`] so every path the
    /// estate's config names is inside the boundary satz confines to; `satz_open` gets
    /// the absolute `config.toml` and the absolute main file.
    pub async fn open(
        bin: &SatzBinary,
        dir: EstateDir,
        main: PathBuf,
        allow: Allow,
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
        let root = session_root(&dir, &main);
        let mcp =
            McpSession::open(bin, &root, allow, utf8(&dir.config_path)?, utf8(&main)?).await?;
        let cli = SatzCli::new(bin.clone(), dir.dir.clone());
        Ok(Arc::new(EstateSession {
            dir,
            main,
            cli,
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
    pub fn tools(&self) -> &[ToolInfo] {
        self.mcp.tools()
    }
    pub fn tool_info(&self, name: &str) -> Option<&ToolInfo> {
        self.mcp.tool(name)
    }
    pub fn instructions(&self) -> &str {
        self.mcp.instructions()
    }
    pub fn guide(&self) -> &str {
        self.mcp.guide()
    }
    /// The `satz mcp` child's stderr, line by line, from now on.
    pub fn mcp_stderr(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.mcp.stderr()
    }
    /// The last lines the `satz mcp` child wrote to stderr — what a view shows before
    /// it follows [`Self::mcp_stderr`].
    pub fn mcp_stderr_backlog(&self) -> Vec<String> {
        self.mcp.stderr_backlog()
    }

    /// Call a tool on this estate's session.
    pub async fn tool(
        &self,
        name: &str,
        args: serde_json::Map<String, serde_json::Value>,
    ) -> Result<ToolOutcome, SatzError> {
        self.mcp.call(name, args).await
    }

    /// Every writer takes this first: a view edit, an interview answer, an agent's
    /// write tool. Held across the check and the rename.
    pub async fn write_lock(&self) -> MutexGuard<'_, ()> {
        self.write_lock.lock().await
    }

    /// `apply` and `bootstrap` run in the user's own terminal: a one-shot script under
    /// the app's data directory (`<data dir>/run/<unix millis>.sh`, `.cmd` on Windows)
    /// holding `cd "<estate>" && "<satz>" --config . <args…>`, opened with the OS
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

/// The root `satz mcp` is confined to: the longest common directory prefix of the
/// estate's directory, every directory its resolved config names (`yaml_dir`,
/// `hcl_dir`, `schema_dir`, `presets_dir`, each `include_dirs` entry) and the main
/// file's directory. For an estate whose config stays inside its directory this is the
/// directory itself. `.` and `..` components are folded lexically, so a directory that
/// does not exist yet (an `hcl_dir` before the first transpile) still counts.
pub fn session_root(dir: &EstateDir, main: &Path) -> PathBuf {
    let runtime = &dir.runtime;
    let mut dirs: Vec<&Path> = vec![&dir.dir];
    dirs.extend(
        [
            &runtime.yaml_dir,
            &runtime.hcl_dir,
            &runtime.schema_dir,
            &runtime.presets_dir,
        ]
        .into_iter()
        .chain(&runtime.include_dirs)
        .map(Path::new),
    );
    dirs.extend(main.parent());
    let normalized: Vec<PathBuf> = dirs.iter().map(|p| normalize(p)).collect();
    let mut prefix: Vec<Component<'_>> = normalized[0].components().collect();
    for p in &normalized[1..] {
        let common = prefix
            .iter()
            .zip(p.components())
            .take_while(|(a, b)| *a == b)
            .count();
        prefix.truncate(common);
    }
    prefix.iter().collect()
}

/// Fold `.` and `..` without touching the filesystem.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(c);
                }
            }
            other => out.push(other),
        }
    }
    out
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

/// `cd "<dir>" && "<satz>" --config . <args…>` as a script for the platform's shell.
fn script_text(dir: &Path, satz: &Path, args: &[String]) -> String {
    if cfg!(windows) {
        let args = args
            .iter()
            .map(|a| cmd_quote(a))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "@echo off\r\ncd /d \"{}\" && \"{}\" --config . {args}\r\n",
            dir.display(),
            satz.display()
        )
    } else {
        let args = args
            .iter()
            .map(|a| sh_quote(a))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "#!/bin/sh\nset -e\ncd \"{}\" && \"{}\" --config . {args}\n",
            dir.display(),
            satz.display()
        )
    }
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

    fn estate_in(dir: &Path, config: &str) -> EstateDir {
        std::fs::write(dir.join("config.toml"), config).unwrap();
        EstateDir::open(dir).unwrap()
    }

    #[test]
    fn a_normal_estate_is_rooted_at_its_own_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = estate_in(tmp.path(), "yaml_dir = \"yaml\"\n");
        let main = tmp.path().join("yaml").join("C0example.satz");
        assert_eq!(session_root(&dir, &main), tmp.path());
    }

    #[test]
    fn the_fixture_is_rooted_at_the_repository() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let dir = EstateDir::open(&repo.join("tests").join("fixtures").join("smoke")).unwrap();
        let main = PathBuf::from(&dir.runtime.yaml_dir).join("smoke.satz");
        assert_eq!(session_root(&dir, &main), normalize(&repo));
    }

    #[test]
    fn a_config_that_reaches_up_widens_the_root() {
        let tmp = tempfile::tempdir().unwrap();
        let estate = tmp.path().join("fleet").join("one");
        std::fs::create_dir_all(&estate).unwrap();
        let dir = estate_in(&estate, "presets_dir = \"../../presets\"\n");
        let main = estate.join("yaml").join("C0example.satz");
        assert_eq!(session_root(&dir, &main), tmp.path());
    }

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
            assert!(text.contains("cd \"/estates/acme\" && \"/usr/local/bin/satz\" --config . apply \"a b\" \"x\\\"y\"\n"), "{text}");
        }
    }
}
