//! Running the satz binary as a command: every call passes `--config <dir>` so paths
//! resolve against the estate, stdout and stderr are streamed line by line, and
//! `--format json` output is typed. This is for what MCP does not serve; the session
//! ([`super::McpSession`]) is for what it does.

use std::path::PathBuf;
use std::process::ExitStatus;

use serde::de::DeserializeOwned;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{SatzBinary, SatzError};

/// One line of a running command, as it arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliLine {
    Stdout(String),
    Stderr(String),
}

#[derive(Debug, Clone)]
pub struct SatzCli {
    pub bin: SatzBinary,
    /// the estate's directory — passed as `--config`, and the working directory
    pub config_dir: PathBuf,
}

impl SatzCli {
    pub fn new(bin: SatzBinary, config_dir: PathBuf) -> Self {
        Self { bin, config_dir }
    }

    /// Run `satz --config <dir> <args…>`, streaming every line into `out`; the exit
    /// status is the result. `cancel` kills the child.
    pub async fn run(&self, args: &[String], out: mpsc::Sender<CliLine>, cancel: CancellationToken) -> Result<ExitStatus, SatzError> {
        let _ = (args, out, cancel);
        Err(crate::Unimplemented::new("SatzCli::run", "U4").into())
    }

    /// Run a command whose stdout is JSON (`--format json`) and type it; a non-zero
    /// exit is an error carrying stderr, never a value.
    pub async fn json<T: DeserializeOwned>(&self, args: &[String]) -> Result<T, SatzError> {
        let _ = args;
        Err(crate::Unimplemented::new("SatzCli::json", "U4").into())
    }
}
