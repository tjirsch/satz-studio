//! The settings file: `<config dir>/satz-studio/settings.toml`. A missing file means
//! first run and yields the defaults; a file that does not parse is an error, never
//! silently replaced. Nothing here is a credential: the app holds none (ADR 0020).

use std::path::{Path, PathBuf};

use crate::agent::DEFAULT_AGENT_COMMAND;
use crate::satz::Allow;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    /// an explicit path to the satz binary; otherwise `PATH`, then `~/.local/bin/satz`
    pub satz_binary: Option<PathBuf>,
    /// the satz release newer than the one this build is tested against whose notice the
    /// operator dismissed, as `satz --version` names it; the notice comes back for any other
    /// newer release. It permits and refuses nothing: a newer satz runs either way
    pub dismissed_satz: Option<String>,
    /// the capability ceiling every `satz mcp` this app starts is given, and the one
    /// `satz mcp-config` writes into an agent's configuration
    pub mcp_allow: Allow,
    /// the agentic client the Agent view starts in the estate's directory, as a command
    /// line; its first word is looked up on `PATH`. Empty means none is configured
    pub agent_command: String,
    pub theme: Theme,
    /// the folder the Start screen opened last
    pub last_root: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            satz_binary: None,
            dismissed_satz: None,
            mcp_allow: Allow::ReadWrite,
            agent_command: DEFAULT_AGENT_COMMAND.to_string(),
            theme: Theme::System,
            last_root: None,
        }
    }
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

/// `<data dir>/satz-studio` — the one-shot scripts the app opens in the OS terminal
/// live here, never inside an estate.
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
    fn a_fresh_settings_file_names_the_default_agent_and_a_written_one_is_read_back() {
        assert_eq!(Settings::default().agent_command, DEFAULT_AGENT_COMMAND);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "agent_command = \"code --wait\"\n").unwrap();
        assert_eq!(
            Settings::load_from(&path).unwrap().agent_command,
            "code --wait"
        );
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
        std::fs::write(&path, "mcp_allow = \"read\"\n").unwrap();
        let s = Settings::load_from(&path).unwrap();
        assert_eq!(s.mcp_allow, Allow::Read);
        assert_eq!(s.agent_command, DEFAULT_AGENT_COMMAND);
        assert_eq!(s.theme, Theme::System);
    }
}
