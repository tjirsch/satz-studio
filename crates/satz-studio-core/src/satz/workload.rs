//! Workload packs through the CLI: `satz packs`, `get-pack`, `update-pack` and
//! `detach-pack` on the estate's `packs_dir` (satz's ADR 0083). They run as commands
//! because their source may be a folder outside the estate — `--from <dir>` — which
//! `satz mcp` confines to its root (ADR 0024).

use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use super::reports::{DetachedPack, GetPackReport, UpdatePackReport, WorkloadPacksReport};
use super::{SatzCli, SatzError};

/// Where workload packs are read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackSource {
    /// the `packs/` tree of the satz repository: a network read
    Repository,
    /// a folder holding packs, `--from <dir>`
    Folder(PathBuf),
}

impl PackSource {
    fn args(&self) -> Vec<String> {
        match self {
            PackSource::Repository => Vec::new(),
            PackSource::Folder(dir) => vec!["--from".to_string(), dir.display().to_string()],
        }
    }

    /// What the window calls it.
    pub fn label(&self) -> String {
        match self {
            PackSource::Repository => "the satz repository".to_string(),
            PackSource::Folder(dir) => dir.display().to_string(),
        }
    }
}

/// One pack, or every pack the command covers with `--all`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Which {
    One(String),
    All,
}

impl Which {
    fn args(&self) -> Vec<String> {
        match self {
            Which::One(id) => vec![id.clone()],
            Which::All => vec!["--all".to_string()],
        }
    }
}

fn argv(command: &str, parts: [Vec<String>; 3]) -> Vec<String> {
    std::iter::once(command.to_string())
        .chain(parts.into_iter().flatten())
        .collect()
}

/// `satz packs [--from <dir>]`: the packs of `packs_dir` with their state, and what the
/// source holds beyond them.
pub async fn list(cli: &SatzCli, source: &PackSource) -> Result<WorkloadPacksReport, SatzError> {
    cli.json_report(&argv("packs", [source.args(), Vec::new(), Vec::new()]))
        .await
}

/// `satz get-pack <id>|--all [--from <dir>]`: the pack fetched into `packs_dir` and its
/// origin recorded.
pub async fn get(
    cli: &SatzCli,
    which: &Which,
    source: &PackSource,
) -> Result<GetPackReport, SatzError> {
    let (report, status) = cli
        .json_stdout(&argv("get-pack", [which.args(), source.args(), Vec::new()]))
        .await?;
    held(
        report,
        status,
        true,
        "get-pack",
        "it installed the packs it names",
    )
}

/// `satz update-pack <id>|--all [--from <dir>] [--take-upstream]`. A non-zero exit with
/// a report is satz's verdict that a pack needs attention, and the report says so
/// itself; the two are held together.
pub async fn update(
    cli: &SatzCli,
    which: &Which,
    source: &PackSource,
    take_upstream: bool,
) -> Result<UpdatePackReport, SatzError> {
    let flags = if take_upstream {
        vec!["--take-upstream".to_string()]
    } else {
        Vec::new()
    };
    let (report, status): (UpdatePackReport, _) = cli
        .json_stdout(&argv("update-pack", [which.args(), source.args(), flags]))
        .await?;
    let clean = !report.attention;
    let says = if clean {
        "no pack needs attention"
    } else {
        "a pack needs attention"
    };
    held(report, status, clean, "update-pack", says)
}

/// `satz detach-pack <id>`: the pack's row removed, its file left.
pub async fn detach(cli: &SatzCli, id: &str) -> Result<DetachedPack, SatzError> {
    let (report, status) = cli
        .json_stdout(&argv(
            "detach-pack",
            [vec![id.to_string()], Vec::new(), Vec::new()],
        ))
        .await?;
    held(report, status, true, "detach-pack", "it removed the row")
}

/// The report, when the exit status says what the report says: a zero exit exactly when
/// the report is a clean one.
fn held<T>(
    report: T,
    status: ExitStatus,
    clean: bool,
    command: &str,
    says: &str,
) -> Result<T, SatzError> {
    if status.success() == clean {
        Ok(report)
    } else {
        Err(SatzError::Verdict {
            command: command.to_string(),
            status,
            report: says.to_string(),
        })
    }
}

/// Whether `dir` may be a source: a folder that is there.
pub fn folder(dir: &Path) -> Option<PackSource> {
    dir.is_dir().then(|| PackSource::Folder(dir.to_path_buf()))
}
