//! Conversations, kept OUTSIDE the estate: `<data dir>/satz-studio/transcripts/
//! <sha256 of the estate path>/<rfc3339>.jsonl`, one message per line. A transcript
//! names projects and ids, and satz's privacy gate rejects local files in an estate
//! repository, so nothing of this ever sits beside an estate.

use std::path::{Path, PathBuf};

use crate::llm::Message;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TranscriptHeader {
    pub estate: PathBuf,
    pub model: String,
    /// RFC 3339
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
    Io { path: PathBuf, #[source] source: std::io::Error },
    #[error("{path}: line {line} is not a message: {source}")]
    Line { path: PathBuf, line: usize, #[source] source: serde_json::Error },
    #[error(transparent)]
    Settings(#[from] crate::settings::SettingsError),
    #[error(transparent)]
    Unimplemented(#[from] crate::Unimplemented),
}

#[derive(Debug, Clone)]
pub struct TranscriptStore {
    pub root: PathBuf,
}

impl TranscriptStore {
    /// `<data dir>/satz-studio/transcripts`
    pub fn open_default() -> Result<Self, TranscriptError> {
        Ok(Self { root: crate::settings::data_dir()?.join("transcripts") })
    }

    /// The directory of one estate's transcripts.
    pub fn dir_for(&self, estate: &Path) -> PathBuf {
        self.root.join(crate::edit::sha256_hex(estate.to_string_lossy().as_bytes()))
    }

    pub fn list(&self, estate: &Path) -> Result<Vec<PathBuf>, TranscriptError> {
        let _ = estate;
        Err(crate::Unimplemented::new("TranscriptStore::list", "U6").into())
    }
    pub fn create(&self, estate: &Path, model: &str) -> Result<Transcript, TranscriptError> {
        let _ = (estate, model);
        Err(crate::Unimplemented::new("TranscriptStore::create", "U6").into())
    }
    pub fn load(&self, path: &Path) -> Result<Transcript, TranscriptError> {
        let _ = path;
        Err(crate::Unimplemented::new("TranscriptStore::load", "U6").into())
    }
    /// Append one message to the transcript's file.
    pub fn append(&self, transcript: &mut Transcript, message: Message) -> Result<(), TranscriptError> {
        let _ = (transcript, message);
        Err(crate::Unimplemented::new("TranscriptStore::append", "U6").into())
    }
}
