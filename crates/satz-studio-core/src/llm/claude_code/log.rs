//! The Claude Code stream log: every line one session exchanged with the CLI, verbatim
//! and in order, so a turn that failed can be read back as what arrived rather than as
//! what the parser made of it.
//!
//! One file per session — one conversation — under `<data dir>/satz-studio/logs/
//! claude-code/`, named after the instant it opened. Each record is one line:
//!
//! ```text
//! <RFC 3339 instant>\t<channel>\t<the line, byte for byte>
//! ```
//!
//! `stdout` is a line the CLI wrote, recorded before it is parsed; `stdin` a line the app
//! wrote to it; `stderr` a line of the CLI's standard error; `studio` the app's own note —
//! the header naming the session, the error a turn ended with, the cap. Nothing is
//! redacted: the lines carry the estate's contents, its resource names and whatever the
//! operator typed, which is why the log is off by default, local, and bounded — at most
//! [`MAX_BYTES`] per file and [`MAX_FILES`] files, the oldest deleted when a session opens
//! one more. The stream of a session comes back with
//! `awk -F'\t' '$2 == "stdout"' <file> | cut -f3-`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::cli::ClaudeCodeError;

/// A log file stops recording at 16 MiB, and says so in its last line.
pub const MAX_BYTES: u64 = 16 * 1024 * 1024;
/// The ten newest session logs are kept.
pub const MAX_FILES: usize = 10;

/// Room kept under the size bound for the line that says the bound was reached.
const CAP_NOTE_RESERVE: u64 = 256;

/// Where the log goes and how far it may grow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamLogConfig {
    pub dir: PathBuf,
    pub max_bytes: u64,
    pub max_files: usize,
}

impl StreamLogConfig {
    /// `dir` with the app's bounds.
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            max_bytes: MAX_BYTES,
            max_files: MAX_FILES,
        }
    }

    /// `<data dir>/satz-studio/logs/claude-code` — beside the transcripts, never inside
    /// an estate.
    pub fn default_dir() -> Result<PathBuf, crate::settings::SettingsError> {
        Ok(crate::settings::data_dir()?
            .join("logs")
            .join("claude-code"))
    }
}

/// Which side of the session a record is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// a line the app wrote to the CLI
    Stdin,
    /// a line the CLI wrote, before any parsing
    Stdout,
    /// a line of the CLI's standard error
    Stderr,
    /// the app's own note
    Studio,
}

impl Channel {
    pub fn tag(self) -> &'static str {
        match self {
            Channel::Stdin => "stdin",
            Channel::Stdout => "stdout",
            Channel::Stderr => "stderr",
            Channel::Studio => "studio",
        }
    }
}

/// One session's log file, shared by the session and the task reading the CLI's stderr.
#[derive(Debug)]
pub struct StreamLog {
    path: PathBuf,
    max_bytes: u64,
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    file: std::fs::File,
    written: u64,
    /// the cap was reached and said so; later records are not written
    full: bool,
    /// a write failed; every later record returns this failure
    broken: Option<(std::io::ErrorKind, String)>,
}

impl StreamLog {
    /// Open a new log in `config.dir`, delete the oldest logs so that at most
    /// `config.max_files` remain with this one, and write `header` as its first record.
    pub fn create(
        config: &StreamLogConfig,
        header: &serde_json::Value,
    ) -> Result<StreamLog, ClaudeCodeError> {
        let dir = &config.dir;
        std::fs::create_dir_all(dir).map_err(|e| io(format!("creating {}", dir.display()), e))?;
        let existing = Self::list(dir)?;
        let keep = config.max_files.saturating_sub(1);
        for old in existing.iter().skip(keep) {
            std::fs::remove_file(old)
                .map_err(|e| io(format!("deleting the old log {}", old.display()), e))?;
        }
        let created = crate::transcript::rfc3339_now();
        let path = dir.join(format!("{}.log", created.replace(':', "-")));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| io(format!("creating {}", path.display()), e))?;
        let log = StreamLog {
            path,
            max_bytes: config.max_bytes,
            state: Mutex::new(State {
                file,
                written: 0,
                full: false,
                broken: None,
            }),
        };
        log.record(Channel::Studio, &header.to_string())?;
        Ok(log)
    }

    /// The session logs in `dir`, newest first. Only files named as [`StreamLog::create`]
    /// names them are counted; a directory that is not there holds none.
    pub fn list(dir: &Path) -> Result<Vec<PathBuf>, ClaudeCodeError> {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io(format!("listing {}", dir.display()), e)),
        };
        let mut logs = Vec::new();
        for entry in entries {
            let path = entry
                .map_err(|e| io(format!("listing {}", dir.display()), e))?
                .path();
            let named = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(is_log_name);
            if named && path.is_file() {
                logs.push(path);
            }
        }
        logs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
        Ok(logs)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one record. A record that would take the file past its bound is replaced
    /// by one line saying the bound was reached, and nothing after it is written. A
    /// write that fails is returned, and so is every record after it.
    pub fn record(&self, channel: Channel, line: &str) -> Result<(), ClaudeCodeError> {
        let mut state = self
            .state
            .lock()
            .expect("the log lock is never poisoned: nothing panics while holding it");
        if let Some((kind, message)) = &state.broken {
            return Err(self.failed(std::io::Error::new(*kind, message.clone())));
        }
        if state.full {
            return Ok(());
        }
        let now = crate::transcript::rfc3339_now();
        let mut text = format!("{now}\t{}\t{line}\n", channel.tag());
        let limit = self.max_bytes.saturating_sub(CAP_NOTE_RESERVE);
        if state.written + text.len() as u64 > limit {
            text = format!(
                "{now}\t{}\tthe log reached its bound of {} bytes; nothing after this line is recorded\n",
                Channel::Studio.tag(),
                self.max_bytes
            );
            state.full = true;
        }
        let written = state
            .file
            .write_all(text.as_bytes())
            .and_then(|()| state.file.flush());
        match written {
            Ok(()) => {
                state.written += text.len() as u64;
                Ok(())
            }
            Err(e) => {
                state.broken = Some((e.kind(), e.to_string()));
                Err(self.failed(e))
            }
        }
    }

    fn failed(&self, source: std::io::Error) -> ClaudeCodeError {
        io(
            format!("writing the Claude Code log {}", self.path.display()),
            source,
        )
    }
}

/// `2026-09-16T20-43-01.123456Z.log`: what [`StreamLog::create`] writes, and the only
/// files the bound on their number counts or deletes.
fn is_log_name(name: &str) -> bool {
    name.len() > "Z.log".len()
        && name.ends_with("Z.log")
        && name.as_bytes()[0].is_ascii_digit()
        && name.contains('T')
}

fn io(context: String, source: std::io::Error) -> ClaudeCodeError {
    ClaudeCodeError::Io { context, source }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(dir: &Path, max_bytes: u64, max_files: usize) -> StreamLogConfig {
        StreamLogConfig {
            dir: dir.to_path_buf(),
            max_bytes,
            max_files,
        }
    }

    fn records(path: &Path) -> Vec<(String, String)> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|l| {
                let mut parts = l.splitn(3, '\t');
                let _at = parts.next().unwrap();
                (
                    parts.next().unwrap().to_string(),
                    parts.next().unwrap().to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn a_record_is_the_line_byte_for_byte_behind_its_instant_and_its_channel() {
        let tmp = tempfile::tempdir().unwrap();
        let log = StreamLog::create(
            &config(tmp.path(), MAX_BYTES, MAX_FILES),
            &serde_json::json!({"estate": "acme.satz"}),
        )
        .unwrap();
        let raw = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":""}}}"#;
        log.record(Channel::Stdout, raw).unwrap();
        log.record(Channel::Stderr, "a warning\twith a tab")
            .unwrap();
        let text = std::fs::read_to_string(log.path()).unwrap();
        let line = text.lines().nth(1).unwrap();
        let (at, rest) = line.split_once('\t').unwrap();
        assert!(at.ends_with('Z') && at.contains('T'), "{at}");
        assert_eq!(rest, format!("stdout\t{raw}"));
        assert_eq!(
            records(log.path()),
            vec![
                (
                    "studio".to_string(),
                    r#"{"estate":"acme.satz"}"#.to_string()
                ),
                ("stdout".to_string(), raw.to_string()),
                ("stderr".to_string(), "a warning\twith a tab".to_string()),
            ]
        );
    }

    #[test]
    fn a_log_stops_at_its_bound_and_says_so_once() {
        let tmp = tempfile::tempdir().unwrap();
        let log = StreamLog::create(&config(tmp.path(), 1024, MAX_FILES), &serde_json::json!({}))
            .unwrap();
        let line = "x".repeat(100);
        for _ in 0..20 {
            log.record(Channel::Stdout, &line).unwrap();
        }
        let size = std::fs::metadata(log.path()).unwrap().len();
        assert!(size <= 1024, "{size} bytes");
        let records = records(log.path());
        let last = records.last().unwrap();
        assert_eq!(last.0, "studio");
        assert!(
            last.1.contains("reached its bound of 1024 bytes"),
            "{}",
            last.1
        );
        assert_eq!(
            records.iter().filter(|r| r.1.contains("reached")).count(),
            1
        );
    }

    #[test]
    fn opening_a_log_keeps_the_newest_and_deletes_the_rest() {
        let tmp = tempfile::tempdir().unwrap();
        // three older logs and one file the bound does not count
        for name in [
            "2026-09-01T10-00-00.000001Z.log",
            "2026-09-02T10-00-00.000001Z.log",
            "2026-09-03T10-00-00.000001Z.log",
        ] {
            std::fs::write(tmp.path().join(name), "old\n").unwrap();
        }
        std::fs::write(tmp.path().join("notes.txt"), "mine\n").unwrap();
        let log =
            StreamLog::create(&config(tmp.path(), MAX_BYTES, 3), &serde_json::json!({})).unwrap();
        let kept = StreamLog::list(tmp.path()).unwrap();
        assert_eq!(kept.len(), 3, "{kept:?}");
        assert_eq!(kept[0], log.path());
        let names: Vec<_> = kept
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(!names.contains(&"2026-09-01T10-00-00.000001Z.log".to_string()));
        assert!(tmp.path().join("notes.txt").is_file());
    }

    #[test]
    fn a_directory_that_is_not_there_holds_no_log() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            StreamLog::list(&tmp.path().join("none"))
                .unwrap()
                .is_empty()
        );
    }
}
