//! Workload packs through the CLI: `satz packs`, `get-pack`, `update-pack` and
//! `detach-pack` on the estate's `packs_dir` (satz's ADR 0083), and the pack sources they
//! read, `satz pack-source list|add|remove` (satz's ADR 0097). They run as commands
//! because a source may be a folder outside the estate, which `satz mcp` confines to its
//! root (ADR 0024). A source is named: `satz`, the satz repository's `packs/`, or a
//! `[[pack_source]]` of `config.toml`, which satz alone writes.

use std::path::{Component, Path, PathBuf};
use std::process::ExitStatus;

use super::reports::{
    DetachedPack, GetPackReport, PackSourcesReport, UpdatePackReport, WorkloadPacksReport,
};
use super::{SatzCli, SatzError};

/// A pack source to add to `config.toml` with `satz pack-source add`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSource {
    /// what `origin.toml` records; satz judges it
    pub name: String,
    pub place: NewPlace,
}

/// Where a new pack source is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewPlace {
    /// `--github owner/repository`, with `--ref` and `--path` where given
    Github {
        repo: String,
        git_ref: Option<String>,
        path: Option<String>,
    },
    /// `--dir`: a folder chosen on this machine, written relative to `config.toml`
    Folder(PathBuf),
}

impl NewSource {
    /// `pack-source add <name> …`. A folder is passed relative to `estate_dir`, the
    /// directory of `config.toml`, so every clone laid out alike reads the same folder; a
    /// folder with no relative way there (another drive) is passed as it was chosen.
    pub fn argv(&self, estate_dir: &Path) -> Vec<String> {
        let mut argv = vec![
            "pack-source".to_string(),
            "add".to_string(),
            self.name.clone(),
        ];
        match &self.place {
            NewPlace::Github {
                repo,
                git_ref,
                path,
            } => {
                argv.extend(["--github".to_string(), repo.clone()]);
                if let Some(r) = git_ref {
                    argv.extend(["--ref".to_string(), r.clone()]);
                }
                if let Some(p) = path {
                    argv.extend(["--path".to_string(), p.clone()]);
                }
            }
            NewPlace::Folder(dir) => {
                let written = relative(estate_dir, dir).unwrap_or_else(|| dir.clone());
                argv.extend(["--dir".to_string(), written.display().to_string()]);
            }
        }
        argv
    }
}

/// `to` as a path relative to `from`, both absolute: `None` when they share no root.
fn relative(from: &Path, to: &Path) -> Option<PathBuf> {
    let from: Vec<Component> = from.components().collect();
    let to: Vec<Component> = to.components().collect();
    if from.first() != to.first() {
        return None;
    }
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for part in &to[common..] {
        out.push(part.as_os_str());
    }
    Some(if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    })
}

/// `satz pack-source list --format json`: every pack source, the built-in `satz` first.
/// It reads `config.toml` and nothing else, so it is no network read.
pub async fn sources(cli: &SatzCli) -> Result<PackSourcesReport, SatzError> {
    let (report, status) = cli
        .json_stdout(&["pack-source".to_string(), "list".to_string()])
        .await?;
    held(
        report,
        status,
        true,
        "pack-source list",
        "it lists the sources",
    )
}

/// `satz pack-source add …`: the source written into `config.toml` by satz. What satz
/// prints is the answer; a refusal is an error carrying satz's stderr.
pub async fn add_source(cli: &SatzCli, source: &NewSource) -> Result<String, SatzError> {
    Ok(cli.output(&source.argv(&cli.config_dir)).await?.stdout)
}

/// `satz pack-source remove <name>`, refused by satz while `origin.toml` names it.
pub async fn remove_source(cli: &SatzCli, name: &str) -> Result<String, SatzError> {
    Ok(cli
        .output(&[
            "pack-source".to_string(),
            "remove".to_string(),
            name.to_string(),
        ])
        .await?
        .stdout)
}

/// `--source <name>`.
fn source_args(source: &str) -> Vec<String> {
    vec!["--source".to_string(), source.to_string()]
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

/// `satz packs --source <name>`: the packs of `packs_dir` with their state, and what the
/// source holds beyond them. A GitHub source is a network read.
pub async fn list(cli: &SatzCli, source: &str) -> Result<WorkloadPacksReport, SatzError> {
    cli.json_report(&argv(
        "packs",
        [source_args(source), Vec::new(), Vec::new()],
    ))
    .await
}

/// `satz get-pack <id>|--all --source <name>`: the pack fetched into `packs_dir` and its
/// origin recorded, naming the source.
pub async fn get(cli: &SatzCli, which: &Which, source: &str) -> Result<GetPackReport, SatzError> {
    let (report, status) = cli
        .json_stdout(&argv(
            "get-pack",
            [which.args(), source_args(source), Vec::new()],
        ))
        .await?;
    held(
        report,
        status,
        true,
        "get-pack",
        "it installed the packs it names",
    )
}

/// `satz update-pack <id>|--all [--source <name>] [--take-upstream]`: each pack read
/// from the source its row in `origin.toml` names — only the packs fetched from `only`,
/// where it names one, the others reported as not checked. A non-zero exit with a report
/// is satz's verdict that a pack needs attention, and the report says so itself; the two
/// are held together.
pub async fn update(
    cli: &SatzCli,
    which: &Which,
    only: Option<&str>,
    take_upstream: bool,
) -> Result<UpdatePackReport, SatzError> {
    let mut flags = only.map(source_args).unwrap_or_default();
    if take_upstream {
        flags.push("--take-upstream".to_string());
    }
    let (report, status): (UpdatePackReport, _) = cli
        .json_stdout(&argv("update-pack", [which.args(), flags, Vec::new()]))
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

/// Whether `id` can name a new pack of the repository: a lowercase letter, then
/// lowercase letters, digits and `-`. It is the file's name in `packs_dir` and, with
/// `_` for `-`, the name its `pack` statement declares. `Err` says what is wrong.
pub fn new_pack_id(id: &str) -> Result<(), String> {
    let mut chars = id.chars();
    match chars.next() {
        None => Err("a pack needs a name".to_string()),
        Some(c) if !c.is_ascii_lowercase() => {
            Err(format!("`{id}` does not start with a lowercase letter"))
        }
        _ if chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') => Ok(()),
        _ => Err(format!(
            "`{id}` holds more than lowercase letters, digits and -"
        )),
    }
}

/// The text a new pack starts as: its `pack` statement at version 0.1 and an empty
/// `params` block, which `satz fmt` leaves as it is and an estate that uses it
/// transpiles with.
pub fn new_pack_text(id: &str) -> String {
    format!(
        "pack {} version \"0.1\"\n\nparams {{\n}}\n",
        id.replace('-', "_")
    )
}

/// A new pack of the repository's own, `packs_dir/<id>.satz`, written with
/// [`new_pack_text`] — refused when the file is there — and formatted by `satz fmt`. It
/// has no row in `origin.toml`, so no fetch and no update ever writes it. A file `satz
/// fmt` refuses is removed again, since nothing but this call wrote it.
pub async fn create_pack(cli: &SatzCli, packs_dir: &Path, id: &str) -> Result<PathBuf, String> {
    new_pack_id(id)?;
    std::fs::create_dir_all(packs_dir).map_err(|e| format!("{}: {e}", packs_dir.display()))?;
    let file = packs_dir.join(format!("{id}.satz"));
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => {
                format!("{} is there already", file.display())
            }
            _ => format!("{}: {e}", file.display()),
        })?;
    std::io::Write::write_all(&mut out, new_pack_text(id).as_bytes())
        .map_err(|e| format!("{}: {e}", file.display()))?;
    drop(out);
    let fmt = cli
        .output(&["fmt".to_string(), file.display().to_string()])
        .await;
    if let Err(e) = fmt {
        let _ = std::fs::remove_file(&file);
        return Err(format!("satz fmt refused the new pack: {e}"));
    }
    Ok(file)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_github_source_is_added_with_the_parts_given() {
        let bare = NewSource {
            name: "team".into(),
            place: NewPlace::Github {
                repo: "acme/packs".into(),
                git_ref: None,
                path: None,
            },
        };
        assert_eq!(
            bare.argv(Path::new("/e/estate")),
            ["pack-source", "add", "team", "--github", "acme/packs"]
        );
        let full = NewSource {
            name: "team".into(),
            place: NewPlace::Github {
                repo: "acme/packs".into(),
                git_ref: Some("v1".into()),
                path: Some("workloads".into()),
            },
        };
        assert_eq!(
            full.argv(Path::new("/e/estate")),
            [
                "pack-source",
                "add",
                "team",
                "--github",
                "acme/packs",
                "--ref",
                "v1",
                "--path",
                "workloads"
            ]
        );
    }

    #[test]
    fn a_new_pack_is_named_in_lowercase_and_declares_its_name_with_underscores() {
        assert_eq!(new_pack_id("team-access2"), Ok(()));
        assert_eq!(new_pack_id("a"), Ok(()));
        for wrong in [
            "",
            "Team",
            "2team",
            "-team",
            "team_access",
            "team.access",
            "team/x",
        ] {
            assert!(new_pack_id(wrong).is_err(), "{wrong}");
        }
        assert_eq!(
            new_pack_text("team-access"),
            "pack team_access version \"0.1\"\n\nparams {\n}\n"
        );
    }

    #[test]
    fn a_folder_source_is_written_relative_to_config_toml() {
        let estate = Path::new("/e/work/estate");
        let at = |dir: &str| {
            NewSource {
                name: "local".into(),
                place: NewPlace::Folder(PathBuf::from(dir)),
            }
            .argv(estate)
            .last()
            .cloned()
            .unwrap()
        };
        // the separator is the platform's
        let joined = |parts: &[&str]| parts.iter().collect::<PathBuf>().display().to_string();
        assert_eq!(at("/e/work/packs"), joined(&["..", "packs"]));
        assert_eq!(at("/e/work/estate/packs-up"), "packs-up");
        assert_eq!(at("/e/work/estate"), ".");
        assert_eq!(
            at("/e/other/team/packs"),
            joined(&["..", "..", "other", "team", "packs"])
        );
    }
}
