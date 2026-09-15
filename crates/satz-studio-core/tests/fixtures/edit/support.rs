//! Shared by every `edit_*` test: the smoke estate copied into a temporary directory
//! with a `config.toml` whose paths point into `vendor/satz` absolutely, a session on
//! it, the two checkers, and the node lookups the edits need. Each test file includes
//! it with `#[path = "fixtures/edit/support.rs"]` and uses the part it needs.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use satz_studio_core::cst::{Cst, NodeId, NodeKind};
use satz_studio_core::edit::{CliChecker, McpChecker};
use satz_studio_core::estate::EstateDir;
use satz_studio_core::satz::reports::InterviewReport;
use satz_studio_core::satz::{Allow, EstateSession, SatzBinary};

pub const TIME_BOX: Duration = Duration::from_secs(60);

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// A temporary directory on the REPOSITORY's drive rather than the system one. A
/// session's root is the longest common prefix of the estate's directory and every
/// directory its config names, and those reach into `vendor/satz`; on Windows the
/// system temporary directory is often on another drive, where a temporary estate and
/// the submodule share no prefix at all and there is no root to confine `satz mcp` to.
pub fn scratch() -> tempfile::TempDir {
    let dir = repo_root().join("target").join("test-scratch");
    std::fs::create_dir_all(&dir).unwrap();
    tempfile::Builder::new()
        .prefix("estate")
        .tempdir_in(&dir)
        .unwrap()
}

/// satz's own smoke estates, read straight from the pinned submodule.
pub fn smoke_yaml() -> PathBuf {
    repo_root()
        .join("vendor")
        .join("satz")
        .join("tests")
        .join("smoke")
        .join("yaml")
}

/// The smoke estate where a test may write: `yaml/*.satz` copied, a `config.toml`
/// naming the presets, the schema and the include directory of `vendor/satz` by
/// absolute path. The directory lives as long as the value.
pub struct SmokeCopy {
    _dir: tempfile::TempDir,
    pub root: PathBuf,
}

pub fn copy_smoke() -> SmokeCopy {
    copy_smoke_at(None)
}

/// The same copy at a validation level other than satz's default (`warn`): under
/// `"error"` a finding that would be a warning refuses the compile instead.
pub fn copy_smoke_at(validation_level: Option<&str>) -> SmokeCopy {
    let dir = scratch();
    let root = dir.path().canonicalize().unwrap();
    let yaml = root.join("yaml");
    std::fs::create_dir_all(&yaml).unwrap();
    for entry in std::fs::read_dir(smoke_yaml()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("satz") {
            std::fs::copy(&path, yaml.join(path.file_name().unwrap())).unwrap();
        }
    }
    let vendor = repo_root().join("vendor").join("satz");
    let mut config = format!(
        "yaml_dir = \"yaml\"\nhcl_dir = \"hcl\"\ninclude_dirs = [\".\", \"yaml\", '{}']\nschema_dir = '{}'\npresets_dir = '{}'\ntf_tool = \"tofu\"\nprovider_version = \"7.14.1\"\n",
        vendor.display(),
        vendor.join("tests").join("schemas").display(),
        vendor.join("presets").display(),
    );
    if let Some(level) = validation_level {
        config.push_str(&format!("validation_level = \"{level}\"\n"));
    }
    std::fs::write(root.join("config.toml"), config).unwrap();
    SmokeCopy { _dir: dir, root }
}

impl SmokeCopy {
    pub fn yaml(&self) -> PathBuf {
        self.root.join("yaml")
    }
    pub fn file(&self, name: &str) -> PathBuf {
        self.yaml().join(name)
    }
    /// Every temp file the write discipline could have left behind in `yaml/`.
    pub fn temp_files(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = std::fs::read_dir(self.yaml())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".studio-tmp.satz"))
            })
            .collect();
        out.sort();
        out
    }
    /// A session on `main` (a file name inside `yaml/`), with write capability.
    pub async fn open(&self, main: &str) -> Arc<EstateSession> {
        let bin = SatzBinary::locate(None).await.unwrap();
        let dir = EstateDir::open(&self.root).unwrap();
        within(EstateSession::open(
            &bin,
            dir,
            PathBuf::from(main),
            Allow::ReadWrite,
        ))
        .await
        .unwrap()
    }
}

pub fn checkers(session: &Arc<EstateSession>) -> (McpChecker, CliChecker) {
    (
        McpChecker {
            session: Arc::clone(session),
        },
        CliChecker {
            cli: session.cli.clone(),
        },
    )
}

/// `f` within the time box every async test keeps.
pub async fn within<F: Future>(f: F) -> F::Output {
    tokio::time::timeout(TIME_BOX, f)
        .await
        .expect("finished within the time box")
}

/// `satz_interview {answers}` through the session; a refusal fails the test.
pub async fn interview(session: &EstateSession, answers: serde_json::Value) -> InterviewReport {
    let mut args = serde_json::Map::new();
    args.insert("answers".to_string(), answers);
    let outcome = within(session.tool("satz_interview", args)).await.unwrap();
    assert!(!outcome.is_error, "{}", outcome.text);
    outcome.typed("satz_interview").unwrap()
}

/// Whether `params { }` binds `name` to `value`. By param, not by spacing: satz's own
/// writer keeps a formatted file formatted, so the `=` column of a bound line is the
/// block's, not the writer's.
pub fn binds(text: &str, name: &str, value: &str) -> bool {
    let cst = Cst::parse(text).expect("the file parses");
    cst.param(name)
        .is_some_and(|entry| match cst.node(entry).kind {
            NodeKind::ParamEntry { value: node, .. } => cst.slice(cst.node(node).span) == value,
            _ => false,
        })
}

/// The value node of the param `name` binds.
pub fn param_value(cst: &Cst, name: &str) -> NodeId {
    let entry = cst.param(name).unwrap_or_else(|| panic!("no param {name}"));
    match cst.node(entry).kind {
        NodeKind::ParamEntry { value, .. } => value,
        ref other => panic!("{other:?}"),
    }
}

/// The first attribute named `key`, wherever the fixture puts it — a test that
/// pinned a line number broke the day satz moved a line in its own fixture.
pub fn attr_named(cst: &Cst, key: &str) -> NodeId {
    cst.nodes()
        .find(|(_, n)| matches!(n.kind, NodeKind::Attr { key: k, .. } if cst.slice(k) == key))
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("no attribute {key}"))
}

/// The 1-based line whose trimmed text starts with `needle`.
pub fn line_of(cst: &Cst, needle: &str) -> u32 {
    cst.text()
        .lines()
        .position(|l| l.trim_start().starts_with(needle))
        .map(|i| i as u32 + 1)
        .unwrap_or_else(|| panic!("no line starting with {needle}"))
}

/// The first value that starts on `line` — a list item, when the line holds one.
pub fn value_on_line(cst: &Cst, line: u32) -> NodeId {
    cst.nodes_at_line(line)
        .into_iter()
        .find(|&id| matches!(cst.node(id).kind, NodeKind::Value(_)))
        .unwrap_or_else(|| panic!("no value on line {line}"))
}

pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}
