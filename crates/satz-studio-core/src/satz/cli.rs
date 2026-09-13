//! Running the satz binary as a command: every call passes `--config <dir>` so paths
//! resolve against the estate, stdout and stderr are streamed line by line, and
//! `--format json` output is typed. This is for what MCP does not serve; the session
//! ([`super::McpSession`]) is for what it does.

use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};

use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
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

    /// `satz --config <dir> <args…>` in the estate's directory, killed when dropped.
    fn command(&self, args: &[String]) -> Command {
        let mut cmd = Command::new(&self.bin.path);
        cmd.arg("--config")
            .arg(&self.config_dir)
            .args(args)
            .current_dir(&self.config_dir)
            .kill_on_drop(true);
        cmd
    }

    /// Run `satz --config <dir> <args…>`, streaming every line into `out`; the exit
    /// status is the result. `cancel` kills the child and the result is
    /// [`SatzError::Cancelled`]. A receiver that has gone ends the forwarding, not the
    /// process: the pipes are drained so the child can finish.
    pub async fn run(
        &self,
        args: &[String],
        out: mpsc::Sender<CliLine>,
        cancel: CancellationToken,
    ) -> Result<ExitStatus, SatzError> {
        let command = args.join(" ");
        let mut child = self
            .command(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| SatzError::Io {
                context: format!("spawning `satz {command}`"),
                source: e,
            })?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let pump_out = tokio::spawn(pump(stdout, out.clone(), CliLine::Stdout));
        let pump_err = tokio::spawn(pump(stderr, out, CliLine::Stderr));

        let status = tokio::select! {
            () = cancel.cancelled() => {
                if let Err(e) = child.kill().await {
                    // `kill` refuses a child that has already exited; that child is
                    // reaped below, and anything else is a real failure.
                    let exited = child.try_wait().map_err(|e| SatzError::Io { context: format!("waiting for `satz {command}`"), source: e })?.is_some();
                    if !exited {
                        return Err(SatzError::Io { context: format!("killing `satz {command}`"), source: e });
                    }
                }
                join_pumps(pump_out, pump_err, &command).await?;
                return Err(SatzError::Cancelled);
            }
            status = child.wait() => status.map_err(|e| SatzError::Io { context: format!("waiting for `satz {command}`"), source: e })?,
        };
        join_pumps(pump_out, pump_err, &command).await?;
        Ok(status)
    }

    /// Run a command whose stdout is JSON (`--format json`) and type it; a non-zero
    /// exit is an error carrying stderr, never a value.
    pub async fn json<T: DeserializeOwned>(&self, args: &[String]) -> Result<T, SatzError> {
        let command = args.join(" ");
        let output = self
            .command(args)
            .output()
            .await
            .map_err(|e| SatzError::Io {
                context: format!("running `satz {command}`"),
                source: e,
            })?;
        if !output.status.success() {
            return Err(SatzError::Exit {
                command,
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        serde_json::from_slice(&output.stdout).map_err(|e| SatzError::Json { command, source: e })
    }
}

/// Forward one pipe line by line. Once the receiver is gone the lines are read and
/// dropped, so the child never blocks on a full pipe.
async fn pump<R: AsyncRead + Unpin>(
    pipe: R,
    out: mpsc::Sender<CliLine>,
    wrap: fn(String) -> CliLine,
) -> std::io::Result<()> {
    let mut lines = BufReader::new(pipe).lines();
    let mut forwarding = true;
    while let Some(line) = lines.next_line().await? {
        if forwarding && out.send(wrap(line)).await.is_err() {
            forwarding = false;
        }
    }
    Ok(())
}

async fn join_pumps(
    pump_out: tokio::task::JoinHandle<std::io::Result<()>>,
    pump_err: tokio::task::JoinHandle<std::io::Result<()>>,
    command: &str,
) -> Result<(), SatzError> {
    for (name, handle) in [("stdout", pump_out), ("stderr", pump_err)] {
        let context = format!("reading the {name} of `satz {command}`");
        handle
            .await
            .map_err(|e| SatzError::Io {
                context: context.clone(),
                source: std::io::Error::other(e),
            })?
            .map_err(|e| SatzError::Io { context, source: e })?;
    }
    Ok(())
}
