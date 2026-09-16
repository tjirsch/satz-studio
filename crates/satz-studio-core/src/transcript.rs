//! Conversations, kept OUTSIDE the estate: `<data dir>/satz-studio/transcripts/
//! <sha256 of the estate path>/<created>.jsonl` — line 1 the header, then one message
//! per line. A transcript names projects and ids, and satz's privacy gate rejects local
//! files in an estate repository, so nothing of this ever sits beside an estate.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::llm::Message;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TranscriptHeader {
    pub estate: PathBuf,
    pub model: String,
    /// RFC 3339, UTC, microseconds
    pub created: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub path: PathBuf,
    pub header: TranscriptHeader,
    pub messages: Vec<Message>,
}

#[derive(Debug, thiserror::Error)]
pub enum TranscriptError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: line {line} is not a message: {source}")]
    Line {
        path: PathBuf,
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error(transparent)]
    Settings(#[from] crate::settings::SettingsError),
}

#[derive(Debug, Clone)]
pub struct TranscriptStore {
    pub root: PathBuf,
}

impl TranscriptStore {
    /// `<data dir>/satz-studio/transcripts`
    pub fn open_default() -> Result<Self, TranscriptError> {
        Ok(Self {
            root: crate::settings::data_dir()?.join("transcripts"),
        })
    }

    /// The directory of one estate's transcripts.
    pub fn dir_for(&self, estate: &Path) -> PathBuf {
        self.root
            .join(crate::edit::sha256_hex(estate.to_string_lossy().as_bytes()))
    }

    /// The estate's transcripts, newest first. An estate without a directory has none.
    pub fn list(&self, estate: &Path) -> Result<Vec<PathBuf>, TranscriptError> {
        let dir = self.dir_for(estate);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(TranscriptError::Io {
                    path: dir,
                    source: e,
                });
            }
        };
        let mut paths = Vec::new();
        for entry in entries {
            let path = entry
                .map_err(|e| TranscriptError::Io {
                    path: dir.clone(),
                    source: e,
                })?
                .path();
            if path.extension().is_some_and(|x| x == "jsonl") {
                paths.push(path);
            }
        }
        paths.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
        Ok(paths)
    }

    /// A new transcript, its file named after its creation time. A file that already
    /// exists is never overwritten.
    pub fn create(&self, estate: &Path, model: &str) -> Result<Transcript, TranscriptError> {
        let dir = self.dir_for(estate);
        std::fs::create_dir_all(&dir).map_err(|e| TranscriptError::Io {
            path: dir.clone(),
            source: e,
        })?;
        let created = rfc3339_now();
        let path = dir.join(format!("{}.jsonl", created.replace(':', "-")));
        let header = TranscriptHeader {
            estate: estate.to_path_buf(),
            model: model.to_string(),
            created,
        };
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| TranscriptError::Io {
                path: path.clone(),
                source: e,
            })?;
        write_line(&mut file, &path, &header)?;
        Ok(Transcript {
            path,
            header,
            messages: Vec::new(),
        })
    }

    /// Read a transcript back: line 1 the header, every other line a message. A line
    /// that is neither is [`TranscriptError::Line`].
    pub fn load(&self, path: &Path) -> Result<Transcript, TranscriptError> {
        let text = std::fs::read_to_string(path).map_err(|e| TranscriptError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        let mut lines = text.lines().enumerate();
        let (_, first) = lines.next().ok_or_else(|| TranscriptError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "an empty file is not a transcript",
            ),
        })?;
        let header: TranscriptHeader =
            serde_json::from_str(first).map_err(|e| TranscriptError::Line {
                path: path.to_path_buf(),
                line: 1,
                source: e,
            })?;
        let mut messages = Vec::new();
        for (i, line) in lines {
            if line.trim().is_empty() {
                continue;
            }
            messages.push(
                serde_json::from_str(line).map_err(|e| TranscriptError::Line {
                    path: path.to_path_buf(),
                    line: i + 1,
                    source: e,
                })?,
            );
        }
        Ok(Transcript {
            path: path.to_path_buf(),
            header,
            messages,
        })
    }

    /// Append one message to the transcript's file, flushed before it returns.
    pub fn append(
        &self,
        transcript: &mut Transcript,
        message: Message,
    ) -> Result<(), TranscriptError> {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&transcript.path)
            .map_err(|e| TranscriptError::Io {
                path: transcript.path.clone(),
                source: e,
            })?;
        write_line(&mut file, &transcript.path, &message)?;
        transcript.messages.push(message);
        Ok(())
    }
}

fn write_line<T: serde::Serialize>(
    file: &mut std::fs::File,
    path: &Path,
    value: &T,
) -> Result<(), TranscriptError> {
    let mut line = serde_json::to_string(value).expect("a header or a message serialises");
    line.push('\n');
    file.write_all(line.as_bytes())
        .and_then(|()| file.flush())
        .map_err(|e| TranscriptError::Io {
            path: path.to_path_buf(),
            source: e,
        })
}

/// Now, as `YYYY-MM-DDTHH:MM:SS.ffffffZ`.
pub(crate) fn rfc3339_now() -> String {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970");
    rfc3339(since.as_secs(), since.subsec_micros())
}

fn rfc3339(secs: u64, micros: u32) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{micros:06}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Days since 1970-01-01 to a proleptic Gregorian date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_renders_known_instants() {
        assert_eq!(rfc3339(0, 0), "1970-01-01T00:00:00.000000Z");
        assert_eq!(rfc3339(951_782_400, 5), "2000-02-29T00:00:00.000005Z");
        assert_eq!(
            rfc3339(1_789_000_000, 123_456),
            "2026-09-10T00:26:40.123456Z"
        );
    }
}
