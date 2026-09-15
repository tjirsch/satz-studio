//! Running the satz binary as a command: a call on an estate passes `--config <dir>` so
//! paths resolve against it, stdout and stderr are streamed line by line, and a
//! reporting command's JSON is read from the file it wrote. [`SatzCli::run_in`] is the
//! one call that has no estate to point at yet. This is for what MCP does not serve;
//! the session ([`super::McpSession`]) is for what it does.

use std::path::{Path, PathBuf};
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
        stream(self.command(args), args.join(" "), out, cancel).await
    }

    /// Run `satz <args…>` **in `dir`, with no `--config`**, streaming every line into
    /// `out` exactly as [`Self::run`] does.
    ///
    /// `satz init` is what this exists for, and it is an associated function because at
    /// that moment there is no estate to build a [`SatzCli`] around. `init` is the
    /// command that WRITES `config.toml`, so the file every other call points at does
    /// not exist yet: satz refuses `--config <dir>` for a directory holding no
    /// `config.toml`, and the working directory is the whole of the address. Run there,
    /// `init` creates `config.toml`, `yaml/`, `hcl/`, `schemas/`, `.gitignore` and the
    /// estate file relative to it.
    pub async fn run_in(
        bin: &SatzBinary,
        dir: &Path,
        args: &[String],
        out: mpsc::Sender<CliLine>,
        cancel: CancellationToken,
    ) -> Result<ExitStatus, SatzError> {
        let mut cmd = Command::new(&bin.path);
        cmd.args(args).current_dir(dir).kill_on_drop(true);
        stream(cmd, args.join(" "), out, cancel).await
    }

    /// Run a reporting command and type the report it wrote. A reporting command takes
    /// one format and writes one file (satz's ADR 0021): `args` is the command and its
    /// own arguments, and this appends `--format json` and an `--out` of its own — a
    /// file in a temporary directory that is removed when the call returns, whichever
    /// way it returns. A non-zero exit is an error carrying stderr, never a value; so
    /// is an exit that wrote no file.
    pub async fn json_report<T: DeserializeOwned>(&self, args: &[String]) -> Result<T, SatzError> {
        let dir = tempfile::Builder::new()
            .prefix("satz-studio-report")
            .tempdir()
            .map_err(|e| SatzError::Io {
                context: "making a directory for a satz report".to_string(),
                source: e,
            })?;
        let out = dir.path().join("report.json");
        let mut argv = args.to_vec();
        argv.extend([
            "--format".to_string(),
            "json".to_string(),
            "--out".to_string(),
            out.display().to_string(),
        ]);
        let command = argv.join(" ");
        let output = self
            .command(&argv)
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
        let written = tokio::fs::read(&out).await.map_err(|e| SatzError::Io {
            context: format!("reading the report `satz {command}` wrote"),
            source: e,
        })?;
        serde_json::from_slice(&written).map_err(|e| SatzError::Json { command, source: e })
    }
}

/// Spawn one satz child and stream both its pipes into `out` until it exits.
/// `command` is what the call looks like on the command line, for the error messages.
async fn stream(
    mut cmd: Command,
    command: String,
    out: mpsc::Sender<CliLine>,
    cancel: CancellationToken,
) -> Result<ExitStatus, SatzError> {
    let mut child = cmd
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
