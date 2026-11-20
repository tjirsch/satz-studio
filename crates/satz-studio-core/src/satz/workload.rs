//! Workload packs through the CLI: `satz packs`, `get-pack`, `update-pack` and
//! `detach-pack` on the estate's `packs_dir` (satz's ADR 0083), `publish-pack`, which
//! copies one of the repository's own packs into a folder source (satz's ADR 0099), and
//! the pack sources they read, `satz pack-source list|add|remove` (satz's ADR 0097). They run as commands
//! because a source may be a folder outside the estate, which `satz mcp` confines to its
//! root (ADR 0024). A source is named: `satz`, the satz repository's `packs/`, or a
//! `[[pack_source]]` of `config.toml`, which satz alone writes.

use std::path::{Component, Path, PathBuf};
use std::process::ExitStatus;

use super::reports::{
    DetachedPack, GetPackReport, PackPlace, PackSourcesReport, PublishAction, Published,
    UpdatePackReport, WorkloadPacksReport,
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

/// `--estate <file>`: the estate a pack's reads are matched against. The open estate is
/// passed always, so a `yaml_dir` that holds several is no refusal.
fn estate_args(estate: &Path) -> Vec<String> {
    vec!["--estate".to_string(), estate.display().to_string()]
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

/// `satz packs --source <name> --estate <estate>`: the packs of `packs_dir` with their
/// state, and what the source holds beyond them, each with what it declares — params,
/// list params, exports — and its interface reads matched against `estate` (satz's ADR
/// 0100). A GitHub source is a network read.
pub async fn list(
    cli: &SatzCli,
    source: &str,
    estate: &Path,
) -> Result<WorkloadPacksReport, SatzError> {
    cli.json_report(&argv(
        "packs",
        [source_args(source), estate_args(estate), Vec::new()],
    ))
    .await
}

/// `satz packs --local --estate <estate>`: the packs of `packs_dir` only, each with what it
/// declares and its reads matched against `estate`, and the interface folders `estate`
/// uses. No pack source is read, so it is no network read: a fetched pack is `edited` or
/// `not-checked` by the sha256 its row in `origin.toml` records, and `sources` is empty.
pub async fn list_local(cli: &SatzCli, estate: &Path) -> Result<WorkloadPacksReport, SatzError> {
    cli.json_report(&argv(
        "packs",
        [vec!["--local".to_string()], estate_args(estate), Vec::new()],
    ))
    .await
}

/// `satz get-pack <id>|--all --source <name> --estate <estate>`: the pack fetched into
/// `packs_dir` and its origin recorded, naming the source; each installed pack names the
/// reads `estate` does not give, which satz warns of and fetches all the same.
pub async fn get(
    cli: &SatzCli,
    which: &Which,
    source: &str,
    estate: &Path,
) -> Result<GetPackReport, SatzError> {
    let (report, status) = cli
        .json_stdout(&argv(
            "get-pack",
            [which.args(), source_args(source), estate_args(estate)],
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

/// The arguments of `satz publish-pack <id> --source <name> --estate <estate> [--force]`;
/// `--format json` is added where it runs.
pub fn publish_argv(id: &str, source: &str, estate: &Path, force: bool) -> Vec<String> {
    let mut argv = vec![
        "publish-pack".to_string(),
        id.to_string(),
        "--source".to_string(),
        source.to_string(),
        "--estate".to_string(),
        estate.display().to_string(),
    ];
    if force {
        argv.push("--force".to_string());
    }
    argv
}

/// `satz publish-pack`: one of the repository's own packs copied into the folder source
/// `source`, after it clears `review-pack`'s bar inside `estate`, and recorded in
/// `origin.toml` as fetched from there. `force` copies it over another file of that id.
/// A refusal is an error carrying satz's stderr, which [`PublishRefusal::read`] reads.
pub async fn publish(
    cli: &SatzCli,
    id: &str,
    source: &str,
    estate: &Path,
    force: bool,
) -> Result<Published, SatzError> {
    let (report, status) = cli
        .json_stdout(&publish_argv(id, source, estate, force))
        .await?;
    held(
        report,
        status,
        true,
        "publish-pack",
        "it published the pack",
    )
}

/// The sources `publish-pack` writes: the folders `config.toml` names. A GitHub source,
/// the built-in `satz` among them, is written by a pull request to its repository.
pub fn folder_sources(report: &PackSourcesReport) -> Vec<(String, String)> {
    report
        .sources
        .iter()
        .filter_map(|s| match &s.place {
            PackPlace::Folder(dir) => Some((s.name.clone(), dir.clone())),
            PackPlace::Github { .. } => None,
        })
        .collect()
}

/// What a publish did, in one line: where the file went, or that it was there already.
pub fn published_text(p: &Published) -> String {
    match p.action {
        PublishAction::Copied => format!("{} {} copied to {}", p.id, p.version, p.path),
        PublishAction::Identical => format!(
            "{} {} already identical in {} ({}); its origin is recorded",
            p.id, p.version, p.source, p.path
        ),
        PublishAction::Overwritten => {
            format!("{} {} overwritten at {}", p.id, p.version, p.path)
        }
    }
}

/// A `publish-pack` refusal as the Packs view answers it, read from satz's sentence
/// ([`super::project::sentence`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishRefusal {
    /// the source's folder holds another file of that id: `--force` copies over `file`
    Exists { file: String },
    /// the pack does not clear `review-pack`'s bar: one line per error, and the command
    /// satz names that lists every finding
    Bar {
        lines: Vec<String>,
        command: Option<String>,
    },
    /// anything else, shown as satz said it
    Other,
}

impl PublishRefusal {
    pub fn read(said: &str) -> PublishRefusal {
        let first = said.lines().next().unwrap_or_default();
        if let Some((file, _)) = first.split_once(" holds another `")
            && first.contains("--force")
        {
            return PublishRefusal::Exists {
                file: file.to_string(),
            };
        }
        if first.contains("does not clear review-pack's bar") {
            let rest: Vec<&str> = said.lines().skip(1).collect();
            let lines = rest
                .iter()
                .filter(|l| l.starts_with("  "))
                .map(|l| l.trim().to_string())
                .collect();
            let command = rest
                .iter()
                .find(|l| l.starts_with("`satz review-pack "))
                .and_then(|l| l.split('`').nth(1))
                .map(str::to_string);
            return PublishRefusal::Bar { lines, command };
        }
        PublishRefusal::Other
    }
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
    fn a_publish_names_the_pack_the_source_and_the_estate_and_forces_only_when_asked() {
        let estate = Path::new("/e/yaml/wp.satz");
        assert_eq!(
            publish_argv("team-notes", "shared", estate, false),
            [
                "publish-pack",
                "team-notes",
                "--source",
                "shared",
                "--estate",
                "/e/yaml/wp.satz"
            ]
        );
        let forced = publish_argv("team-notes", "shared", estate, true);
        assert_eq!(forced.last().map(String::as_str), Some("--force"));
        assert_eq!(forced.len(), 7);
    }

    #[test]
    fn only_a_folder_source_is_offered_to_publish_to() {
        use crate::satz::reports::PackSourceRow;
        let report = PackSourcesReport {
            sources: vec![
                PackSourceRow {
                    name: "satz".into(),
                    builtin: true,
                    place: PackPlace::Github {
                        repo: "tjirsch/satz".into(),
                        git_ref: Some("main".into()),
                        path: Some("packs".into()),
                    },
                },
                PackSourceRow {
                    name: "team".into(),
                    builtin: false,
                    place: PackPlace::Github {
                        repo: "acme/packs".into(),
                        git_ref: None,
                        path: None,
                    },
                },
                PackSourceRow {
                    name: "shared".into(),
                    builtin: false,
                    place: PackPlace::Folder("../shared".into()),
                },
            ],
        };
        assert_eq!(
            folder_sources(&report),
            [("shared".to_string(), "../shared".to_string())]
        );
        assert!(
            folder_sources(&PackSourcesReport {
                sources: report.sources[..2].to_vec()
            })
            .is_empty()
        );
    }

    #[test]
    fn a_publish_says_where_the_file_went() {
        let mut p = Published {
            id: "team-notes".into(),
            version: "1.0".into(),
            source: "shared".into(),
            location: "../shared".into(),
            path: "../shared/team-notes.satz".into(),
            action: PublishAction::Copied,
            estate: "yaml/wp.satz".into(),
        };
        assert_eq!(
            published_text(&p),
            "team-notes 1.0 copied to ../shared/team-notes.satz"
        );
        p.action = PublishAction::Identical;
        assert_eq!(
            published_text(&p),
            "team-notes 1.0 already identical in shared (../shared/team-notes.satz); its origin is recorded"
        );
        p.action = PublishAction::Overwritten;
        assert_eq!(
            published_text(&p),
            "team-notes 1.0 overwritten at ../shared/team-notes.satz"
        );
    }

    /// The refusals as satz words them (`src/workload_packs.rs` of satz v0.106.0).
    #[test]
    fn a_publish_refusal_is_read_for_what_the_view_offers() {
        let exists = "./../shared/team-notes.satz holds another `team-notes` than ./packs/team-notes.satz — compare the two files; `--force` copies this one over it";
        assert_eq!(
            PublishRefusal::read(exists),
            PublishRefusal::Exists {
                file: "./../shared/team-notes.satz".into()
            }
        );
        let bar = "./packs/team-notes.satz does not clear review-pack's bar inside /e/yaml/wp.satz, so it is not published:\n  line 1: no header comment — open with one sentence saying what it does\n  /e/yaml/wp.satz writes nothing from this pack — does that estate `use` it?\n`satz review-pack ./packs/team-notes.satz --against /e/yaml/wp.satz --format text --out -` lists every finding";
        assert_eq!(
            PublishRefusal::read(bar),
            PublishRefusal::Bar {
                lines: vec![
                    "line 1: no header comment — open with one sentence saying what it does"
                        .into(),
                    "/e/yaml/wp.satz writes nothing from this pack — does that estate `use` it?"
                        .into(),
                ],
                command: Some(
                    "satz review-pack ./packs/team-notes.satz --against /e/yaml/wp.satz --format text --out -"
                        .into()
                ),
            }
        );
        assert_eq!(
            PublishRefusal::read(
                "the pack `x` came from `shared`; detach it first (`satz detach-pack x`) to publish it as your own"
            ),
            PublishRefusal::Other
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
