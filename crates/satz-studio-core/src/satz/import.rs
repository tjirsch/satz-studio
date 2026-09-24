//! `satz import`: an estate made out of what already exists.
//!
//! `import` needs a project. Run in a directory that holds no `config.toml` it refuses
//! — *Config file 'config.toml' not found in current directory* — because it imports
//! INTO an estate rather than creating one. So the door is two steps: a directory that
//! is already an estate takes the import alone, and one that is not takes `satz init`
//! first, which writes `config.toml` and the directories the import then writes into.
//! [`ImportPlan`] is that decision, made from the directory and nothing else.
//!
//! The SOURCE decides the shape ([`ImportShape`]) and the shape decides the flags:
//! `--on-collision`, `--only`, `--exclude`, `--all`, `--customer-shortname` and
//! `--output` belong to a state file or a live scope, `--organization` to a state file
//! alone, `--wrap-all` to Terraform HCL.
//! [`ImportOptions::argv`]
//! renders exactly the flags of the chosen shape, so no form can send satz a flag it
//! would ignore.
//!
//! What the run WROTE is read back rather than predicted ([`written_since`]): the file
//! name differs by shape — `discovered.satz` from a state file or a live scope,
//! `imported-hcl.satz` from Terraform — and
//! `--output` moves it again. And what the run FOUND is its console output:
//! `satz import` writes no JSON report, so [`ImportReport`] splits the lines it streamed
//! into the sections an operator acts on, which is what makes a dropped, skipped or
//! wrapped resource visible instead of silent.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::edit::sha256_hex;
use crate::estate::{EstateDir, EstateError, declares_an_estate, is_checked_temp};

use super::{CliLine, SatzError};

/// What the source is, which is what decides the flags.
///
/// The form's own value is [`Self::as_str`]; `--from` takes [`Self::from_flag`], where
/// a live scope is satz's `org`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportShape {
    /// a `tofu show -json` document
    #[default]
    State,
    /// a live scope: `organizations/<n>`, `folders/<n>`, `projects/<id>`, or the import
    /// config's `root` when none is named
    Live,
    /// Terraform HCL: a `.tf` file or the directory holding them
    Hcl,
}

impl ImportShape {
    pub const ALL: [ImportShape; 3] = [ImportShape::State, ImportShape::Live, ImportShape::Hcl];

    /// The value of `--from`. Every run states it: the form knows the shape, so satz is
    /// told rather than left to infer one from a path.
    pub fn from_flag(self) -> &'static str {
        match self {
            ImportShape::State => "state",
            ImportShape::Live => "org",
            ImportShape::Hcl => "hcl",
        }
    }

    /// The shape as the form names it.
    pub fn as_str(self) -> &'static str {
        match self {
            ImportShape::State => "state",
            ImportShape::Live => "live",
            ImportShape::Hcl => "hcl",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        ImportShape::ALL.into_iter().find(|s| s.as_str() == value)
    }
}

/// `--on-collision`: one principal holding a grant on two folders or two projects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnCollision {
    /// refuse the import, naming them
    #[default]
    Error,
    /// write the second and later as labelled resources with a running number
    Counter,
}

impl OnCollision {
    pub const ALL: [OnCollision; 2] = [OnCollision::Error, OnCollision::Counter];

    pub fn as_arg(self) -> &'static str {
        match self {
            OnCollision::Error => "error",
            OnCollision::Counter => "counter",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        OnCollision::ALL.into_iter().find(|c| c.as_arg() == value)
    }
}

/// The arguments of one `satz import` run, as the form holds them.
///
/// Every field is a flag `satz import` has, and a field of a shape other than
/// [`Self::shape`] never reaches the command line. The flags this does NOT carry are
/// the ones the door has no business sending: `--into` (importing only what an open
/// estate does not already declare is growing an estate, not starting one) and
/// `--import-config` (the import table is satz's, and a copy of it is a file the
/// operator passes to satz directly).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportOptions {
    pub shape: ImportShape,
    /// the positional source: a path for the state and hcl shapes, a scope for
    /// the live one, empty for the live shape's "the import config's `root`"
    pub source: String,
    /// `--only`: the resource types to import (state, live)
    pub only: Vec<String>,
    /// `--exclude`: the resource types to leave out (state, live)
    pub exclude: Vec<String>,
    /// `--all`: every type the source can deliver, not only the rows marked
    /// `import: true` (state, live)
    pub all: bool,
    /// `--on-collision` (state, live)
    pub on_collision: OnCollision,
    /// `--customer-shortname`: the one value no platform fact carries (state, live)
    pub customer_shortname: String,
    /// `--organization`: the organisation a state belongs to, for a state that names
    /// none — satz writes no estate from such a state without it (state; a live sweep
    /// reads it from its own root, and satz refuses the flag there)
    pub organization: String,
    /// `--output`: the file inside `yaml_dir`; empty is satz's `discovered.satz`
    /// (state, live)
    pub output: String,
    /// `--verbose`: satz's own global flag, which is what turns the skipped summary's
    /// counts into one line per resource — it names the flag itself in that summary
    /// (state, live)
    pub verbose: bool,
    /// `--wrap-all`: carry every block verbatim inside `hcl trust` (hcl)
    pub wrap_all: bool,
}

impl ImportOptions {
    /// The whole command line after the binary: `import`, the source, `--from`, and the
    /// flags of this shape alone. The order is fixed so the preview a form shows is the
    /// command that runs.
    pub fn argv(&self) -> Vec<String> {
        let mut argv = vec!["import".to_string()];
        let source = self.source.trim();
        if !source.is_empty() {
            argv.push(source.to_string());
        }
        argv.push("--from".to_string());
        argv.push(self.shape.from_flag().to_string());
        match self.shape {
            ImportShape::State | ImportShape::Live => {
                push_list(&mut argv, "--only", &self.only);
                push_list(&mut argv, "--exclude", &self.exclude);
                if self.all {
                    argv.push("--all".to_string());
                }
                argv.push("--on-collision".to_string());
                argv.push(self.on_collision.as_arg().to_string());
                push_value(&mut argv, "--customer-shortname", &self.customer_shortname);
                if self.shape == ImportShape::State {
                    push_value(&mut argv, "--organization", &self.organization);
                }
                push_value(&mut argv, "--output", &self.output);
                if self.verbose {
                    argv.push("--verbose".to_string());
                }
            }
            ImportShape::Hcl => {
                if self.wrap_all {
                    argv.push("--wrap-all".to_string());
                }
            }
        }
        argv
    }

    /// The directories this import writes a `.satz` file into, which is where the run is
    /// read back from: `yaml_dir`, for every shape.
    pub fn write_dirs(&self, estate: &EstateDir) -> Vec<PathBuf> {
        vec![estate.yaml_dir()]
    }

    /// That the source is THERE, and that a live scope is one: the half a form can ask on
    /// every keystroke, because it touches no file's contents. `dir` is the directory the
    /// command runs in, which is what a relative source resolves against.
    pub fn check_source_exists(&self, dir: &Path) -> Result<(), SatzError> {
        let source = self.source.trim();
        match self.shape {
            ImportShape::State => {
                let path = resolve(dir, source);
                if path.is_file() {
                    Ok(())
                } else {
                    Err(SatzError::SourceMissing(path))
                }
            }
            // a Terraform source is a file or the directory holding them
            ImportShape::Hcl => {
                let path = resolve(dir, source);
                if path.exists() {
                    Ok(())
                } else {
                    Err(SatzError::SourceMissing(path))
                }
            }
            // an empty scope is the import config's `root`, which is a source like any
            // other; anything that is not a scope is a typed path or a typo
            ImportShape::Live if source.is_empty() || is_scope(source) => Ok(()),
            ImportShape::Live => Err(SatzError::NotAScope(source.to_string())),
        }
    }

    /// The whole check, run once before a child is spawned: the source is there, and for
    /// the state shape it is the document satz reads.
    ///
    /// `satz import` reads the JSON `tofu show -json` writes, not a raw `.tfstate`, and
    /// the two are told apart by `values.root_module` — a distinction an operator
    /// otherwise learns by failing a run once. It is not part of
    /// [`Self::check_source_exists`] because answering it means parsing the document,
    /// which for a real organisation is megabytes: once per run, never per keystroke.
    /// The form states the requirement under the field instead.
    pub fn check_source(&self, dir: &Path) -> Result<(), SatzError> {
        self.check_source_exists(dir)?;
        if self.shape == ImportShape::State {
            let path = resolve(dir, self.source.trim());
            if !is_show_document(&path)? {
                return Err(SatzError::NotAShowDocument(path));
            }
        }
        Ok(())
    }
}

/// What the Import door does in a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportPlan {
    /// the directory is an estate already: `satz import` runs in it
    Import,
    /// the directory holds no `config.toml`: `satz init` writes one, then `satz import`
    /// fills it
    InitThenImport,
}

/// The plan for `dir`, or the reason there is none.
///
/// The directory must exist — the child is spawned with it as its working directory, and
/// one that is not there is an unreadable spawn failure. A `config.toml` is the whole of
/// the question: `satz import` refuses a directory without one, and `satz init` is what
/// writes it.
pub fn plan(dir: &Path) -> Result<ImportPlan, SatzError> {
    if !dir.is_dir() {
        return Err(SatzError::TargetMissing(dir.to_path_buf()));
    }
    Ok(if dir.join("config.toml").is_file() {
        ImportPlan::Import
    } else {
        ImportPlan::InitThenImport
    })
}

/// One `.satz` file an import run wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub path: PathBuf,
    /// the file declares an estate, so it is one the app can open
    pub declares_estate: bool,
}

/// Every `.satz` file in `dirs`, by path, with a hash of its bytes.
///
/// A directory that is not there contributes nothing rather than failing: on the
/// two-step path this is taken before `satz init` has made `yaml/`.
pub fn satz_files(dirs: &[PathBuf]) -> Result<BTreeMap<PathBuf, String>, EstateError> {
    let mut out = BTreeMap::new();
    for dir in dirs {
        for path in satz_paths(dir)? {
            let bytes = read(&path)?;
            out.insert(path, sha256_hex(&bytes));
        }
    }
    Ok(out)
}

/// The `.satz` files in `dirs` that are new since `before` or whose bytes changed:
/// what the run WROTE, read back instead of predicted.
///
/// The hash rather than the listing is what makes a second import of the same source
/// answer: satz writes `discovered.satz` over the one that was already there, and a
/// listing taken twice would say nothing happened.
pub fn written_since(
    before: &BTreeMap<PathBuf, String>,
    dirs: &[PathBuf],
) -> Result<Vec<Written>, EstateError> {
    let mut out = Vec::new();
    for dir in dirs {
        for path in satz_paths(dir)? {
            let bytes = read(&path)?;
            if before.get(&path) == Some(&sha256_hex(&bytes)) {
                continue;
            }
            let declares_estate = std::str::from_utf8(&bytes).is_ok_and(declares_an_estate);
            out.push(Written {
                path,
                declares_estate,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// satz's own import report, as the run printed it.
///
/// `satz import` has no `--format json`: the report IS its console output, and what it
/// wrote, could not derive and left out is the point of running it. So this SPLITS the
/// lines and never rewrites one — satz's own sentence carries the reason, and anything
/// the app wrote instead would be a worse version of it.
///
/// Two rules, in this order. The stream: everything satz wrote on stdout is the report,
/// and from stderr only a line it marks `warning:`, `error:` or `import:` joins it, the
/// rest being the version banner and what it loaded. Then the prefix: the sections an
/// operator acts on are lifted out by the words satz opens them with — `warning:` and
/// `error:`, and the two losses satz states as `import:` blocks (attributes the provider
/// schema names that the estate does not carry, asset types Cloud Asset Inventory does
/// not serve), each with the indented lines under it; `import: params not derivable:`;
/// and `import: skipped` with the lines under it. A line no prefix claims lands in
/// [`Self::rest`], so a satz that words one differently moves it between sections;
/// nothing is ever dropped, which [`Self::len`] is held to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// `warning:` and `error:` lines, and the two losses satz reports with the lines
    /// under them — `import: N attribute(s) the provider schema names are NOT in the
    /// estate` (an apply would reset them) and `import: N asset type(s) Cloud Asset
    /// Inventory does not serve` (nothing of them is in the estate): what the estate
    /// needs written by hand
    pub warnings: Vec<String>,
    /// `import: params not derivable: <name> — <reason>`: the worklist, with satz's own
    /// reason per param. One of them is `customer_shortname`, which the form's
    /// `--customer-shortname` answers on the next run.
    pub not_derivable: Vec<String>,
    /// `import: skipped N resource(s):` and every line under it — the counts, the
    /// reasons, and the levers satz names. A skipped resource is a finding, so this is
    /// its own section rather than the tail of a log.
    pub skipped: Vec<String>,
    /// the files satz wrote, with what to do next
    pub wrote: Vec<String>,
    /// everything else satz printed, in order: the counts it translated, promoted,
    /// wrapped and dropped, the discovery statistics, what a conversion compiled to
    pub rest: Vec<String>,
}

impl ImportReport {
    pub fn of(lines: &[CliLine]) -> Self {
        let mut report = ImportReport::default();
        // `import: skipped N resource(s):` heads a block: the indented lines under it
        // are its counts, its reasons and (with `--verbose`) one line per resource. A
        // loss satz reports heads one the same way: the indented lines are what was lost
        // and the levers satz names.
        let mut block: Option<Block> = None;
        for line in lines {
            let text = match line {
                CliLine::Stdout(text) if !text.trim().is_empty() => text,
                CliLine::Stderr(text) if claimed_on_stderr(text) => text,
                _ => continue,
            };
            let trimmed = text.trim_start();
            let indented = text.len() != trimmed.len();
            if indented && let Some(block) = block {
                match block {
                    Block::Skipped => report.skipped.push(text.clone()),
                    Block::Loss => report.warnings.push(text.clone()),
                }
                continue;
            }
            block = None;
            if trimmed.starts_with("warning:") || trimmed.starts_with("error:") {
                report.warnings.push(text.clone());
            } else if is_loss(trimmed) {
                block = Some(Block::Loss);
                report.warnings.push(text.clone());
            } else if trimmed.starts_with("import: params not derivable:") {
                report.not_derivable.push(text.clone());
            } else if trimmed.starts_with("import: skipped") {
                block = Some(Block::Skipped);
                report.skipped.push(text.clone());
            } else if !indented && trimmed.starts_with("Wrote ") {
                report.wrote.push(text.clone());
            } else {
                report.rest.push(text.clone());
            }
        }
        report
    }

    /// Every line the report holds: what a caller checks against what satz printed, so
    /// a section that stops matching shows up as a line moved and never as one lost.
    pub fn len(&self) -> usize {
        self.warnings.len()
            + self.not_derivable.len()
            + self.skipped.len()
            + self.wrote.len()
            + self.rest.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The block a heading line opens: what the indented lines under it belong to.
#[derive(Debug, Clone, Copy)]
enum Block {
    Skipped,
    Loss,
}

/// A heading satz prints for something the estate lost, in satz's own words: the
/// attributes the provider schema names that the estate does not carry — an apply would
/// reset them on the live resource — and the asset types Cloud Asset Inventory does not
/// serve, of which nothing is in the estate (`report_skipped` in satz's
/// `src/discovery.rs`).
fn is_loss(trimmed: &str) -> bool {
    trimmed.starts_with("import: ")
        && (trimmed.contains(" attribute(s) the provider schema names are NOT in the estate")
            || trimmed.contains(" asset type(s) Cloud Asset Inventory does not serve"))
}

/// A stderr line satz marks as its own: a finding, or a part of the report it printed
/// there rather than on stdout.
fn claimed_on_stderr(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("warning:")
        || trimmed.starts_with("error:")
        || trimmed.starts_with("import:")
}

/// A path as the child resolves it: absolute as is, relative against the working
/// directory the command runs in.
fn resolve(dir: &Path, source: &str) -> PathBuf {
    let path = PathBuf::from(source);
    if path.is_absolute() {
        path
    } else {
        dir.join(path)
    }
}

/// `organizations/<number>`, `folders/<number>` or `projects/<id>`, as satz reads a
/// live scope.
fn is_scope(source: &str) -> bool {
    for prefix in ["organizations/", "folders/"] {
        if let Some(number) = source.strip_prefix(prefix) {
            return !number.is_empty() && number.chars().all(|c| c.is_ascii_digit());
        }
    }
    source
        .strip_prefix("projects/")
        .is_some_and(|id| !id.is_empty() && !id.contains('/'))
}

/// Whether `path` is the document `tofu show -json` writes: a JSON object carrying
/// `values.root_module`. A raw `.tfstate` is JSON without it, and so is anything else.
fn is_show_document(path: &Path) -> Result<bool, SatzError> {
    #[derive(serde::Deserialize)]
    struct Document {
        #[serde(default)]
        values: Option<Values>,
    }
    #[derive(serde::Deserialize)]
    struct Values {
        #[serde(default)]
        root_module: Option<serde::de::IgnoredAny>,
    }
    let bytes = std::fs::read(path).map_err(|e| SatzError::Io {
        context: format!("reading {}", path.display()),
        source: e,
    })?;
    Ok(serde_json::from_slice::<Document>(&bytes)
        .is_ok_and(|document| document.values.is_some_and(|v| v.root_module.is_some())))
}

/// The `.satz` files in one directory, temp files of the write discipline aside. A
/// directory that is not there is an empty list.
fn satz_paths(dir: &Path) -> Result<Vec<PathBuf>, EstateError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(dir).map_err(|e| EstateError::Io {
        path: dir.to_path_buf(),
        source: e,
    })?;
    let mut out = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| EstateError::Io {
                path: dir.to_path_buf(),
                source: e,
            })?
            .path();
        if path.extension().and_then(|e| e.to_str()) == Some("satz") && !is_checked_temp(&path) {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn read(path: &Path) -> Result<Vec<u8>, EstateError> {
    std::fs::read(path).map_err(|e| EstateError::Io {
        path: path.to_path_buf(),
        source: e,
    })
}

fn push_value(argv: &mut Vec<String>, name: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        argv.push(name.to_string());
        argv.push(value.to_string());
    }
}

fn push_list(argv: &mut Vec<String>, name: &str, list: &[String]) {
    let joined = list
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>()
        .join(",");
    if !joined.is_empty() {
        argv.push(name.to_string());
        argv.push(joined);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// satz's documented example values; no live value is ever written into a test here.
    const ORGANIZATION: &str = "organizations/123456789012";

    fn state(source: &str) -> ImportOptions {
        ImportOptions {
            shape: ImportShape::State,
            source: source.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn the_bare_command_of_each_shape_states_the_shape() {
        assert_eq!(
            state("state.json").argv(),
            [
                "import",
                "state.json",
                "--from",
                "state",
                "--on-collision",
                "error"
            ]
        );
        assert_eq!(
            ImportOptions {
                shape: ImportShape::Live,
                ..Default::default()
            }
            .argv(),
            ["import", "--from", "org", "--on-collision", "error"]
        );
        assert_eq!(
            ImportOptions {
                shape: ImportShape::Hcl,
                source: "src".to_string(),
                ..Default::default()
            }
            .argv(),
            ["import", "src", "--from", "hcl"]
        );
    }

    #[test]
    fn every_state_field_renders_its_own_flag_in_order() {
        let options = ImportOptions {
            shape: ImportShape::State,
            source: "state.json".to_string(),
            only: vec!["google_folder".to_string(), "google_project".to_string()],
            exclude: vec!["google_*_iam_member".to_string()],
            all: true,
            on_collision: OnCollision::Counter,
            customer_shortname: "acme".to_string(),
            organization: "123456789012".to_string(),
            output: "discovery.satz".to_string(),
            verbose: true,
            ..Default::default()
        };
        assert_eq!(
            options.argv(),
            [
                "import",
                "state.json",
                "--from",
                "state",
                "--only",
                "google_folder,google_project",
                "--exclude",
                "google_*_iam_member",
                "--all",
                "--on-collision",
                "counter",
                "--customer-shortname",
                "acme",
                "--organization",
                "123456789012",
                "--output",
                "discovery.satz",
                "--verbose",
            ]
        );
    }

    /// A live sweep reads the organisation from its own root, and satz refuses
    /// `--organization` there: the field the state shape carries never reaches it.
    #[test]
    fn a_live_scope_is_the_source_and_takes_the_same_flags() {
        let options = ImportOptions {
            shape: ImportShape::Live,
            source: ORGANIZATION.to_string(),
            customer_shortname: "acme".to_string(),
            organization: "123456789012".to_string(),
            ..Default::default()
        };
        assert_eq!(
            options.argv(),
            [
                "import",
                ORGANIZATION,
                "--from",
                "org",
                "--on-collision",
                "error",
                "--customer-shortname",
                "acme",
            ]
        );
    }

    /// The whole point of a shape-scoped render: a form that carries every field must
    /// still send only the flags of the shape it is on, because satz would refuse or
    /// ignore the rest.
    #[test]
    fn a_shapes_options_never_leak_into_another_shapes_command_line() {
        let everything = ImportOptions {
            source: "source".to_string(),
            only: vec!["google_folder".to_string()],
            exclude: vec!["google_project".to_string()],
            all: true,
            on_collision: OnCollision::Counter,
            customer_shortname: "acme".to_string(),
            organization: "123456789012".to_string(),
            output: "discovery.satz".to_string(),
            verbose: true,
            wrap_all: true,
            shape: ImportShape::State,
        };
        let state_and_live = [
            "--only",
            "--exclude",
            "--all",
            "--on-collision",
            "--customer-shortname",
            "--output",
            "--verbose",
        ];
        let state = [state_and_live.as_slice(), &["--organization"]].concat();
        let hcl_only = ["--wrap-all"];
        let not_live = [hcl_only.as_slice(), &["--organization"]].concat();
        let not_hcl = state.clone();

        for (shape, mine, others) in [
            (ImportShape::State, state.as_slice(), hcl_only.to_vec()),
            (ImportShape::Live, state_and_live.as_slice(), not_live),
            (ImportShape::Hcl, hcl_only.as_slice(), not_hcl),
        ] {
            let argv = ImportOptions {
                shape,
                ..everything.clone()
            }
            .argv();
            for flag in mine {
                assert!(
                    argv.iter().any(|a| a == flag),
                    "{shape:?} lost {flag}: {argv:?}"
                );
            }
            for flag in others {
                assert!(
                    !argv.iter().any(|a| a == flag),
                    "{shape:?} carried {flag}: {argv:?}"
                );
            }
        }
    }

    #[test]
    fn a_blank_or_whitespace_field_is_not_passed_at_all() {
        let options = ImportOptions {
            shape: ImportShape::State,
            source: "  state.json  ".to_string(),
            customer_shortname: "   ".to_string(),
            organization: " ".to_string(),
            output: String::new(),
            only: vec![String::new(), "  ".to_string()],
            ..Default::default()
        };
        assert_eq!(
            options.argv(),
            [
                "import",
                "state.json",
                "--from",
                "state",
                "--on-collision",
                "error"
            ]
        );
    }

    /// The live shape with no scope is satz's "the import config's `root`", which is a
    /// command line with no positional at all — not an empty string, which would be a
    /// source satz cannot resolve.
    #[test]
    fn a_live_import_without_a_scope_carries_no_positional() {
        let argv = ImportOptions {
            shape: ImportShape::Live,
            ..Default::default()
        }
        .argv();
        assert_eq!(argv[1], "--from");
    }

    #[test]
    fn a_directory_with_a_config_takes_the_import_alone() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("config.toml"), "yaml_dir = \"yaml\"\n").unwrap();
        assert_eq!(plan(tmp.path()).unwrap(), ImportPlan::Import);
    }

    #[test]
    fn a_directory_without_one_takes_init_first() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(plan(tmp.path()).unwrap(), ImportPlan::InitThenImport);
    }

    #[test]
    fn a_directory_that_is_not_there_has_no_plan() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope");
        let err = plan(&missing).unwrap_err();
        assert!(
            matches!(err, SatzError::TargetMissing(ref p) if p == &missing),
            "{err:?}"
        );
    }

    /// The refusal an operator otherwise buys with a failed run: satz reads what
    /// `tofu show -json` writes, and a raw `.tfstate` is JSON that does not carry
    /// `values.root_module` — whatever it is named.
    #[test]
    fn a_raw_tfstate_is_refused_before_anything_runs_whatever_its_name() {
        let tmp = tempfile::tempdir().unwrap();
        let raw = r#"{"version":4,"resources":[{"type":"google_storage_bucket"}]}"#;
        for name in ["state.json", "terraform.tfstate"] {
            std::fs::write(tmp.path().join(name), raw).unwrap();
            let err = state(name).check_source(tmp.path()).unwrap_err();
            assert!(matches!(err, SatzError::NotAShowDocument(_)), "{err:?}");
            assert!(err.to_string().contains("tofu show -json > state.json"));
        }
    }

    /// The split is deliberate and stated: the cheap check answers that the file is
    /// there, and only the full one reads it. A form that called the full check on every
    /// keystroke would parse a real organisation's state document on every keystroke.
    #[test]
    fn the_cheap_check_does_not_read_the_document_and_the_full_one_does() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("state.json"),
            r#"{"version":4,"resources":[]}"#,
        )
        .unwrap();
        let options = state("state.json");
        assert!(options.check_source_exists(tmp.path()).is_ok());
        assert!(matches!(
            options.check_source(tmp.path()).unwrap_err(),
            SatzError::NotAShowDocument(_)
        ));
    }

    #[test]
    fn a_show_document_passes_and_a_missing_file_is_named() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("state.json"),
            r#"{"format_version":"1.0","values":{"root_module":{"resources":[]}}}"#,
        )
        .unwrap();
        assert!(state("state.json").check_source(tmp.path()).is_ok());

        let err = state("gone.json").check_source(tmp.path()).unwrap_err();
        assert!(matches!(err, SatzError::SourceMissing(_)), "{err:?}");
    }

    #[test]
    fn a_live_scope_is_a_scope_or_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let live = |source: &str| ImportOptions {
            shape: ImportShape::Live,
            source: source.to_string(),
            ..Default::default()
        };
        for good in [
            "",
            ORGANIZATION,
            "folders/123456789012",
            "projects/acme-infra-001",
        ] {
            assert!(live(good).check_source(tmp.path()).is_ok(), "{good}");
        }
        for bad in [
            "organizations/",
            "organizations/abc",
            "acme",
            "./state.json",
        ] {
            let err = live(bad).check_source(tmp.path()).unwrap_err();
            assert!(matches!(err, SatzError::NotAScope(_)), "{bad}: {err:?}");
        }
    }

    /// The hcl shape takes a file or the directory holding them, so both pass and
    /// neither is guessed at.
    #[test]
    fn the_hcl_source_may_be_a_file_or_a_directory() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src").join("main.tf"), "").unwrap();
        let hcl = |source: &str| ImportOptions {
            shape: ImportShape::Hcl,
            source: source.to_string(),
            ..Default::default()
        };
        assert!(hcl("src").check_source(tmp.path()).is_ok());
        assert!(hcl("src/main.tf").check_source(tmp.path()).is_ok());
        assert!(matches!(
            hcl("nope").check_source(tmp.path()).unwrap_err(),
            SatzError::SourceMissing(_)
        ));
    }

    fn estate_dir(root: &Path) -> EstateDir {
        std::fs::create_dir_all(root.join("yaml")).unwrap();
        std::fs::write(root.join("config.toml"), "yaml_dir = \"yaml\"\n").unwrap();
        EstateDir::open(root).unwrap()
    }

    /// Where each shape writes, which is where the run is read back from. The yaml
    /// shape is the one that does not write into the estate.
    #[test]
    fn the_write_directory_follows_the_shape() {
        let tmp = tempfile::tempdir().unwrap();
        let estate = estate_dir(tmp.path());
        let yaml_dir = tmp.path().join("yaml");
        for shape in [ImportShape::State, ImportShape::Live, ImportShape::Hcl] {
            let options = ImportOptions {
                shape,
                source: "src".to_string(),
                ..Default::default()
            };
            assert_eq!(options.write_dirs(&estate), [yaml_dir.clone()].as_slice());
        }
    }

    /// The read-back: a file that is new is what the run wrote, and so is one whose
    /// bytes changed — a second import over the same `discovered.satz` must not read as
    /// nothing having happened.
    #[test]
    fn written_since_answers_new_files_and_changed_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let yaml = tmp.path().join("yaml");
        std::fs::create_dir_all(&yaml).unwrap();
        std::fs::write(yaml.join("main.satz"), "estate acme\n").unwrap();
        let dirs = vec![yaml.clone()];
        let before = satz_files(&dirs).unwrap();

        // nothing ran
        assert!(written_since(&before, &dirs).unwrap().is_empty());

        // one file written, one rewritten, one that is not a .satz file
        std::fs::write(yaml.join("discovered.satz"), "estate discovered\n").unwrap();
        std::fs::write(yaml.join("pack.satz"), "google_folder {\n}\n").unwrap();
        std::fs::write(yaml.join("main.satz"), "estate acme\n// changed\n").unwrap();
        std::fs::write(yaml.join("notes.txt"), "not satz").unwrap();

        let written = written_since(&before, &dirs).unwrap();
        assert_eq!(
            written
                .iter()
                .map(|w| (
                    w.path.file_name().unwrap().to_string_lossy().into_owned(),
                    w.declares_estate
                ))
                .collect::<Vec<_>>(),
            [
                ("discovered.satz".to_string(), true),
                ("main.satz".to_string(), true),
                ("pack.satz".to_string(), false),
            ]
        );
    }

    /// The snapshot is taken before `satz init` has made `yaml/` on the two-step path,
    /// so a directory that is not there is an empty answer rather than a failure.
    #[test]
    fn a_write_directory_that_does_not_exist_yet_is_empty_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = vec![tmp.path().join("yaml")];
        assert!(satz_files(&dirs).unwrap().is_empty());
        assert!(written_since(&BTreeMap::new(), &dirs).unwrap().is_empty());
    }

    /// The report of a real `satz import … --from hcl`, run against a four-line
    /// `main.tf` on v0.59.0: the stdout lines are the report, the stderr banner and the
    /// "Loaded" lines are the machinery.
    #[test]
    fn the_hcl_report_keeps_what_was_promoted_and_drops_the_machinery() {
        let lines = vec![
            CliLine::Stderr("satz v0.59.0 (built 2026-09-15 21:47:23)".to_string()),
            CliLine::Stderr("Loaded 1280 resource types from import config".to_string()),
            CliLine::Stdout("Wrote yaml/imported-hcl.satz — review it, then `satz transpile` and `tofu plan` against the source's state: no changes.".to_string()),
            CliLine::Stdout("import: 1 block(s) translated to Satz, 1 promoted to params, 0 wrapped verbatim, 0 dropped (terraform/provider)".to_string()),
            CliLine::Stdout("  promoted   src/main.tf:1 variable \"region\" — param region = \"europe-west1\" (variable default)".to_string()),
        ];
        let report = ImportReport::of(&lines);
        assert_eq!(report.wrote.len(), 1);
        assert!(report.wrote[0].contains("imported-hcl.satz"));
        assert_eq!(report.rest.len(), 2, "{:?}", report.rest);
        assert!(report.rest[1].contains("variable \"region\""));
        assert!(report.warnings.is_empty() && report.skipped.is_empty());
        assert_eq!(
            report.len(),
            3,
            "the two machinery lines are not the report"
        );
    }

    /// The state shape's report, same run: a `warning:` on stderr is a finding, the
    /// params satz could not derive are their own worklist, and the skipped resources
    /// keep their heading, their reasons and the levers together.
    #[test]
    fn the_state_report_splits_the_warnings_the_worklist_and_what_was_skipped() {
        let lines = vec![
            CliLine::Stderr("satz v0.59.0 (built 2026-09-15 21:47:23)".to_string()),
            CliLine::Stdout("Reading infrastructure state...".to_string()),
            CliLine::Stdout("import: params derived: customer_shortname, deployment_engine".to_string()),
            CliLine::Stdout("import: params not derivable: customer_longname — nothing on the platform names it; `satz interview` asks".to_string()),
            CliLine::Stdout("import: params not derivable: default_region — no single region leads among the regional resources".to_string()),
            CliLine::Stderr("warning: no organization id found among the discovered resources — add `customer_organization_id` to `params` by hand".to_string()),
            CliLine::Stdout("Wrote yaml/discovered.satz — review it, then `satz transpile` and `tofu plan`.".to_string()),
            CliLine::Stdout("import: skipped 20 resource(s):".to_string()),
            CliLine::Stdout("      1 parent not imported".to_string()),
            CliLine::Stdout("      19 type(s) filtered by --only/--exclude, not fetched".to_string()),
            CliLine::Stdout("  - google_storage_bucket b — parent not imported: project acme-infra-001".to_string()),
            CliLine::Stdout("  (--verbose lists every one; `import: false` rows, `--all`, `--only` and `--exclude` are the levers)".to_string()),
        ];
        let report = ImportReport::of(&lines);

        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("customer_organization_id"));
        assert_eq!(report.not_derivable.len(), 2);
        assert!(report.not_derivable[1].contains("no single region leads"));
        assert_eq!(report.skipped.len(), 5, "{:?}", report.skipped);
        assert!(report.skipped[0].contains("skipped 20 resource(s)"));
        assert!(report.skipped[4].contains("are the levers"));
        assert_eq!(report.wrote.len(), 1);
        assert_eq!(
            report.rest,
            [
                "Reading infrastructure state...",
                "import: params derived: customer_shortname, deployment_engine",
            ]
        );
    }

    /// The two losses satz reports on a live import, as satz v0.81.0 words them
    /// (`report_skipped` and `unserved_report` in its `src/discovery.rs`), are warnings
    /// with every line under them: what an apply would reset, and the asset types of
    /// which nothing reached the estate with the two levers satz names. The dropped API
    /// vocabulary, which would not plan either way, stays in the rest.
    #[test]
    fn the_losses_satz_reports_are_warnings_with_the_lines_under_them() {
        let lines = vec![
            CliLine::Stderr("satz v0.81.0 (built 2026-09-23 08:20:04)".to_string()),
            CliLine::Stdout("import: 1 asset type(s) Cloud Asset Inventory does not serve — nothing of them is in the estate:".to_string()),
            CliLine::Stdout("  - bigquery.googleapis.com/Table (google_bigquery_table): INVALID_ARGUMENT: asset type is not supported".to_string()),
            CliLine::Stdout("  Refresh the table with `uv run scripts/update_import_config.py --cai-types presets/cai-asset-types.txt`, or leave the row(s) out of the run with --exclude.".to_string()),
            CliLine::Stdout("import: 2 attribute(s) the provider schema names are NOT in the estate — an apply would reset them on the live resource:".to_string()),
            CliLine::Stdout("  - google_storage_bucket acme-logs .iamConfiguration.uniformBucketLevelAccess.enabled — `uniform_bucket_level_access` is set at this level and the asset data says otherwise here".to_string()),
            CliLine::Stdout("  - google_storage_bucket acme-logs .iamConfiguration.bucketPolicyOnly.enabled — `uniform_bucket_level_access` is set at this level and the asset data says otherwise here".to_string()),
            CliLine::Stdout("import: 3 attribute(s) dropped — not in the provider schema (API vocabulary; would not plan):".to_string()),
            CliLine::Stdout("      3 google_project".to_string()),
            CliLine::Stdout("import: skipped 1 resource(s):".to_string()),
            CliLine::Stdout("      1 parent not imported".to_string()),
        ];
        let report = ImportReport::of(&lines);
        assert_eq!(report.warnings.len(), 6, "{:?}", report.warnings);
        assert!(report.warnings[0].contains("Cloud Asset Inventory does not serve"));
        assert!(report.warnings[2].contains("--exclude"));
        assert!(report.warnings[3].contains("an apply would reset them"));
        assert!(report.warnings[5].contains("bucketPolicyOnly"));
        assert_eq!(
            report.rest,
            [
                "import: 3 attribute(s) dropped — not in the provider schema (API vocabulary; would not plan):",
                "      3 google_project",
            ]
        );
        assert_eq!(report.skipped.len(), 2);
        assert_eq!(report.len(), lines.len() - 1);
    }

    /// The split never loses a line: every line satz printed on stdout, and every one it
    /// marked on stderr, is in exactly one section.
    #[test]
    fn every_line_satz_printed_is_in_exactly_one_section() {
        let lines = vec![
            CliLine::Stderr("satz v0.59.0 (built 2026-09-15 21:47:23)".to_string()),
            CliLine::Stdout("--- Discovery Statistics ---".to_string()),
            CliLine::Stdout("  google_folder: 4".to_string()),
            CliLine::Stdout("Total assets discovered: 4".to_string()),
            CliLine::Stdout(String::new()),
            CliLine::Stderr("warning: quota project".to_string()),
            CliLine::Stdout("import: skipped 1 resource(s):".to_string()),
            CliLine::Stdout("      1 parent not imported".to_string()),
            CliLine::Stdout("Wrote yaml/discovered.satz — review it.".to_string()),
        ];
        let printed = lines
            .iter()
            .filter(|l| match l {
                CliLine::Stdout(t) => !t.trim().is_empty(),
                CliLine::Stderr(t) => claimed_on_stderr(t),
            })
            .count();
        let report = ImportReport::of(&lines);
        assert_eq!(report.len(), printed);
        // a line the prefixes do not claim is kept, not dropped
        assert_eq!(report.rest.len(), 3);
        // and the skipped block stays whole, heading and items
        assert_eq!(report.skipped.len(), 2);
    }
}
