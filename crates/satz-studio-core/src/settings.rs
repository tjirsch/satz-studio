//! The settings file: `<config dir>/satz-studio/settings.toml`. A missing file means
//! first run and yields the defaults; a file that does not parse is an error, never
//! silently replaced. No credential is ever stored here — the key lives in the OS
//! keychain (see `llm::auth`).

use std::path::{Path, PathBuf};

use crate::llm::Effort;
use crate::satz::Allow;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    /// an explicit path to the satz binary; otherwise `PATH`, then `~/.local/bin/satz`
    pub satz_binary: Option<PathBuf>,
    /// an explicit path to the Claude Code binary; otherwise `PATH`, then
    /// `~/.local/bin/claude`. Read only when the provider is Claude Code.
    pub claude_code_binary: Option<PathBuf>,
    /// the capability ceiling every `satz mcp` this app starts is given
    pub mcp_allow: Allow,
    /// run non-destructive write tools the agent asks for without an approval card
    pub auto_approve_writes: bool,
    pub provider: ProviderChoice,
    /// the Claude model when the provider is Claude
    pub model: String,
    pub effort: Effort,
    /// server-side refusal fallbacks (`fallbacks: "default"`)
    pub fallbacks: bool,
    pub persist_transcripts: bool,
    pub theme: Theme,
    /// the folder the Estates view opened last
    pub last_root: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            satz_binary: None,
            claude_code_binary: None,
            mcp_allow: Allow::ReadWrite,
            auto_approve_writes: false,
            provider: ProviderChoice::Claude,
            model: "claude-opus-5".to_string(),
            effort: Effort::High,
            fallbacks: true,
            persist_transcripts: true,
            theme: Theme::System,
            last_root: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderChoice {
    Claude,
    /// any endpoint speaking OpenAI's chat-completions API: Ollama, LM Studio, OpenRouter, Gemini's compatible endpoint
    OpenAiCompat {
        base_url: String,
        model: String,
    },
    Ollama {
        base_url: String,
        model: String,
    },
    /// the installed Claude Code CLI, on the user's claude.ai subscription: no API
    /// key, the loop and the tools Claude Code's own (ADR 0010). `model` is what
    /// `--model` is given; `None` leaves Claude Code its own default.
    ClaudeCode {
        model: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error(
        "no configuration directory on this system (XDG_CONFIG_HOME / ~/Library/Application Support / %APPDATA%)"
    )]
    NoConfigDir,
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: not a settings file: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("settings could not be serialised: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// `<config dir>/satz-studio/settings.toml`
pub fn settings_path() -> Result<PathBuf, SettingsError> {
    let dir = dirs::config_dir().ok_or(SettingsError::NoConfigDir)?;
    Ok(dir.join("satz-studio").join("settings.toml"))
}

/// `<data dir>/satz-studio` — transcripts and one-shot command scripts live here,
/// never inside an estate.
pub fn data_dir() -> Result<PathBuf, SettingsError> {
    let dir = dirs::data_dir().ok_or(SettingsError::NoConfigDir)?;
    Ok(dir.join("satz-studio"))
}

impl Settings {
    pub fn load() -> Result<Self, SettingsError> {
        Self::load_from(&settings_path()?)
    }

    /// A missing file is the first run: the defaults. Anything else that fails is an error.
    pub fn load_from(path: &Path) -> Result<Self, SettingsError> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => {
                return Err(SettingsError::Io {
                    path: path.to_path_buf(),
                    source: e,
                });
            }
        };
        toml::from_str(&text).map_err(|e| SettingsError::Parse {
            path: path.to_path_buf(),
            source: e,
        })
    }

    pub fn save(&self) -> Result<(), SettingsError> {
        self.save_to(&settings_path()?)
    }

    pub fn save_to(&self, path: &Path) -> Result<(), SettingsError> {
        let text = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SettingsError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        std::fs::write(path, text).map_err(|e| SettingsError::Io {
            path: path.to_path_buf(),
            source: e,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_through_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let s = Settings::default();
        s.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path).unwrap(), s);
    }

    #[test]
    fn a_missing_file_is_the_first_run() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            Settings::load_from(&dir.path().join("none.toml")).unwrap(),
            Settings::default()
        );
    }

    #[test]
    fn a_broken_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "model = [").unwrap();
        assert!(matches!(
            Settings::load_from(&path),
            Err(SettingsError::Parse { .. })
        ));
    }

    #[test]
    fn a_partial_file_takes_defaults_for_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "fallbacks = false\n[provider]\nkind = \"ollama\"\nbase_url = \"http://localhost:11434\"\nmodel = \"llama3\"\n").unwrap();
        let s = Settings::load_from(&path).unwrap();
        assert!(!s.fallbacks);
        assert_eq!(s.model, "claude-opus-5");
        assert!(matches!(s.provider, ProviderChoice::Ollama { .. }));
    }
}
