//! Opening a file for its author to edit: a pack's file, from the Packs destination's
//! Edit and New pack.
//!
//! The Editor setting is a command line — `zed`, `code`, `subl -n` — run with the file as
//! its last argument. Empty, it is the system's text editor: `open -t` on macOS,
//! `xdg-open` on Linux, `notepad` on Windows. `open` without `-t` asks for the
//! application registered for `.satz`, and with none registered it fails; the text editor
//! is there on every system. The app does not wait for the editor: the command is
//! started and the window goes on, and only a command that cannot start, or one that
//! exits with a failure, is reported.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;

#[derive(Debug, thiserror::Error)]
pub enum EditorError {
    #[error("`{program}` is not on PATH and is not a program at that path: {source}")]
    NotFound {
        program: String,
        #[source]
        source: which::Error,
    },
    #[error("{}: {source}", path.display())]
    Missing {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Launch(String),
}

/// The platform whose text editor an empty Editor setting means. A parameter rather than
/// `cfg!`, so each platform's command line is testable on any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Linux,
    Windows,
}

impl Platform {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    /// The system's text editor, as the words before the file.
    pub fn text_editor(self) -> &'static [&'static str] {
        match self {
            Self::MacOs => &["open", "-t"],
            Self::Linux => &["xdg-open"],
            Self::Windows => &["notepad"],
        }
    }
}

/// The words of a command line: split on whitespace, a double-quoted word kept whole, so
/// a program under a path with a space in it can be named.
pub fn words(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in command.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    out.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(word);
    }
    out
}

/// The command line that opens `file`: the Editor setting's words, or the platform's
/// text editor when it is empty, and the file last.
pub fn argv(editor: &str, platform: Platform, file: &Path) -> Vec<OsString> {
    let mut out: Vec<OsString> = match words(editor) {
        w if w.is_empty() => platform.text_editor().iter().map(OsString::from).collect(),
        w => w.into_iter().map(OsString::from).collect(),
    };
    out.push(file.as_os_str().to_os_string());
    out
}

/// The Editor setting as it is saved: empty, or a command line whose program is on
/// `PATH` or is a program at the path it names. Anything else is refused, never guessed.
pub fn validate(editor: &str) -> Result<(), EditorError> {
    let Some(program) = words(editor).into_iter().next() else {
        return Ok(());
    };
    which::which(&program)
        .map(|_| ())
        .map_err(|source| EditorError::NotFound { program, source })
}

/// `file` with every `.` and `..` resolved by its text — `estates/acme/../../packs/x.satz`
/// is `packs/x.satz` — and checked to be there. Symlinks are left as they are: the file is
/// opened under the name the estate gives it.
pub fn normalize(file: &Path) -> Result<PathBuf, EditorError> {
    let mut out = PathBuf::new();
    for part in file.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(part);
                }
            }
            other => out.push(other),
        }
    }
    std::fs::metadata(&out).map_err(|source| EditorError::Missing {
        path: out.clone(),
        source,
    })?;
    Ok(out)
}

/// The command line as the operator reads it in a message: the words before the file.
pub fn shown(editor: &str, platform: Platform) -> String {
    match words(editor) {
        w if w.is_empty() => platform.text_editor().join(" "),
        _ => editor.trim().to_string(),
    }
}

/// Open `file` in the editor and return once the command has exited — at once for
/// `open -t`, `xdg-open` and an editor's own launcher, when the window closes for one
/// that runs in the foreground. Nothing blocks on it but the task that awaits it. A
/// command that does not start, or that exits with a failure, is [`EditorError::Launch`]
/// with what it wrote to standard error.
pub async fn open(editor: &str, file: &Path) -> Result<(), EditorError> {
    let file = normalize(file)?;
    let argv = argv(editor, Platform::current(), &file);
    let (program, args) = argv.split_first().expect("argv names a program");
    let out = tokio::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| EditorError::Launch(e.to_string()))?;
    if out.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stderr);
    let said = said.lines().map(str::trim).rfind(|l| !l.is_empty());
    Err(EditorError::Launch(match said {
        Some(line) => format!("{} ({line})", out.status),
        None => out.status.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn an_empty_editor_is_the_system_text_editor_on_each_platform() {
        let file = Path::new("/packs/acme.satz");
        assert_eq!(
            argv("", Platform::MacOs, file),
            os(&["open", "-t", "/packs/acme.satz"])
        );
        assert_eq!(
            argv("  ", Platform::Linux, file),
            os(&["xdg-open", "/packs/acme.satz"])
        );
        assert_eq!(
            argv("", Platform::Windows, file),
            os(&["notepad", "/packs/acme.satz"])
        );
        assert_eq!(shown("", Platform::MacOs), "open -t");
    }

    #[test]
    fn a_configured_editor_runs_with_its_arguments_and_the_file_last() {
        let file = Path::new("/packs/acme.satz");
        for platform in [Platform::MacOs, Platform::Linux, Platform::Windows] {
            assert_eq!(
                argv("  subl -n  --add ", platform, file),
                os(&["subl", "-n", "--add", "/packs/acme.satz"])
            );
        }
        assert_eq!(
            argv(
                "\"/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl\" -n",
                Platform::MacOs,
                file
            ),
            os(&[
                "/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl",
                "-n",
                "/packs/acme.satz"
            ])
        );
        assert_eq!(shown(" zed ", Platform::MacOs), "zed");
    }

    #[test]
    fn a_path_through_parent_folders_is_the_file_it_names() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("estates/acme")).unwrap();
        std::fs::create_dir_all(root.path().join("packs")).unwrap();
        std::fs::write(root.path().join("packs/acme.satz"), "").unwrap();
        let through = root.path().join("estates/acme/../.././packs/acme.satz");
        assert_eq!(
            normalize(&through).unwrap(),
            root.path().join("packs/acme.satz")
        );
        assert!(
            !normalize(&through)
                .unwrap()
                .to_string_lossy()
                .contains("..")
        );
        assert!(matches!(
            normalize(&root.path().join("estates/acme/../none.satz")),
            Err(EditorError::Missing { .. })
        ));
    }

    #[test]
    fn an_editor_that_is_not_installed_is_refused_and_one_at_an_absolute_path_is_kept() {
        assert!(validate("").is_ok());
        assert!(validate("   ").is_ok());
        let missing = validate("satz-studio-no-such-editor --wait").unwrap_err();
        assert!(matches!(missing, EditorError::NotFound { .. }));
        assert!(missing.to_string().contains("satz-studio-no-such-editor"));

        let exe = std::env::current_exe().unwrap();
        assert!(exe.is_absolute());
        assert!(validate(&format!("\"{}\" -n", exe.display())).is_ok());
        let gone = exe.with_file_name("satz-studio-no-such-editor");
        assert!(validate(&format!("\"{}\"", gone.display())).is_err());
    }
}
