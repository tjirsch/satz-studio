//! Shared by every `e2e_*` test: a temporary estate directory whose `config.toml`
//! names the presets, the schema and the include directory of `vendor/satz` by
//! absolute path; the skeleton `satz interview --create` writes into it; the answers
//! satz's smoke matrix pipes; the answer path the app takes (`Snapshot::take`,
//! `satz_interview`, `Snapshot::verify` through `McpChecker`); the map line
//! uncommented as the app uncomments it; the model built as the app builds it; and
//! the output of `satz transpile --check` with the banner stripped. Each test file
//! includes it with `#[path = "fixtures/e2e/support.rs"]` and uses the part it needs.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use satz_studio_core::cst::{Cst, NodeKind, Span, UseState, scan_uses};
use satz_studio_core::diag::Diagnostic;
use satz_studio_core::edit::{Committed, McpChecker, Snapshot};
use satz_studio_core::estate::EstateDir;
use satz_studio_core::model::{EstateModel, MAP_PATH};
use satz_studio_core::satz::reports::{InterviewArgs, InterviewReport, QuestionsReport};
use satz_studio_core::satz::{Allow, CliLine, EstateSession, SatzBinary, SatzCli};
use satz_studio_core::schema::ResourceRegistry;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub const TIME_BOX: Duration = Duration::from_secs(120);

/// The seven values `vendor/satz/scripts/smoke.sh` types into the interview
/// (`printf '%s\n' y C0example 123456789012 example.com acme Acme first.admin
/// 012345-6789AB-CDEF01 '' '' …`), in the order the pack asks them. The `y` before them
/// accepts the seven defaults; the empty lines after them accept the two names derived
/// from the short name. Every value is a documented example value.
pub const TYPED: [(&str, &str); 7] = [
    ("customer_id", "C0example"),
    ("customer_organization_id", "123456789012"),
    ("customer_domain", "example.com"),
    ("customer_shortname", "acme"),
    ("customer_longname", "Acme"),
    ("first_admin", "first.admin"),
    ("billing_account_infra", "012345-6789AB-CDEF01"),
];

/// The piped input of the smoke matrix, line by line.
pub fn smoke_input() -> Vec<String> {
    let mut lines = vec!["y".to_string()];
    lines.extend(TYPED.iter().map(|(_, v)| v.to_string()));
    lines.extend(std::iter::repeat_n(String::new(), 9));
    lines
}

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

pub fn vendor() -> PathBuf {
    repo_root().join("vendor").join("satz")
}

/// A temporary estate directory. It lives as long as the value.
pub struct Estate {
    _dir: tempfile::TempDir,
    pub root: PathBuf,
    pub yaml: PathBuf,
}

/// An estate directory with `yaml/` inside it and the presets, the schema and the
/// include directory of `vendor/satz` by absolute path; `validation_level` is satz's
/// default (`warn`) unless given.
pub fn estate_dir(validation_level: Option<&str>) -> Estate {
    let dir = scratch();
    let root = dir.path().canonicalize().unwrap();
    let yaml = root.join("yaml");
    std::fs::create_dir_all(&yaml).unwrap();
    write_config(&root, &yaml, validation_level);
    Estate {
        _dir: dir,
        root,
        yaml,
    }
}

/// A second config directory over the `yaml/` of another estate, at the given
/// validation level: `satz --config <this>` reads the same files under another rule.
pub fn config_over(yaml: &Path, validation_level: &str) -> Estate {
    let dir = scratch();
    let root = dir.path().canonicalize().unwrap();
    write_config(&root, yaml, Some(validation_level));
    Estate {
        _dir: dir,
        root,
        yaml: yaml.to_path_buf(),
    }
}

fn write_config(root: &Path, yaml: &Path, validation_level: Option<&str>) {
    let vendor = vendor();
    let mut config = format!(
        "yaml_dir = '{}'\nhcl_dir = \"hcl\"\ninclude_dirs = [\".\", '{}', '{}']\nschema_dir = '{}'\npresets_dir = '{}'\ntf_tool = \"tofu\"\nprovider_version = \"7.14.1\"\n",
        yaml.display(),
        yaml.display(),
        vendor.display(),
        vendor.join("tests").join("schemas").display(),
        vendor.join("presets").display(),
    );
    if let Some(level) = validation_level {
        config.push_str(&format!("validation_level = \"{level}\"\n"));
    }
    std::fs::write(root.join("config.toml"), config).unwrap();
}

impl Estate {
    pub fn file(&self, name: &str) -> PathBuf {
        self.yaml.join(name)
    }

    /// Every temp file the write discipline could have left behind in `yaml/`.
    pub fn temp_files(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = std::fs::read_dir(&self.yaml)
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
        let bin = satz().await;
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

    /// The CLI runner with this directory as `--config`.
    pub async fn cli(&self) -> SatzCli {
        SatzCli::new(satz().await, self.root.clone())
    }

    /// `satz --config <root> interview <yaml/name> --create` with stdin from
    /// `/dev/null`: the skeleton, nothing answered.
    pub async fn create_skeleton(&self, name: &str) -> PathBuf {
        self.interview_cli(name, &[]).await
    }

    /// The CLI-driven interview: `--create`, and `lines` on stdin, one answer per line.
    pub async fn interview_cli(&self, name: &str, lines: &[String]) -> PathBuf {
        let bin = satz().await;
        let estate = self.file(name);
        let mut child = tokio::process::Command::new(&bin.path)
            .arg("--config")
            .arg(&self.root)
            .arg("interview")
            .arg(&estate)
            .arg("--create")
            .current_dir(&self.root)
            .stdin(if lines.is_empty() {
                Stdio::null()
            } else {
                Stdio::piped()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn satz interview");
        if let Some(mut stdin) = child.stdin.take() {
            let input: String = lines.iter().map(|l| format!("{l}\n")).collect();
            within(stdin.write_all(input.as_bytes())).await.unwrap();
            drop(stdin);
        }
        let out = within(child.wait_with_output()).await.unwrap();
        assert!(
            out.status.success(),
            "satz interview {} --create failed ({}):\n{}\n{}",
            estate.display(),
            out.status,
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        );
        estate
    }
}

pub async fn satz() -> SatzBinary {
    SatzBinary::locate(None).await.unwrap()
}

/// `f` within the time box every async test keeps.
pub async fn within<F: Future>(f: F) -> F::Output {
    tokio::time::timeout(TIME_BOX, f)
        .await
        .expect("finished within the time box")
}

pub fn one_answer(subject: &str, value: serde_json::Value) -> InterviewArgs {
    InterviewArgs {
        answers: BTreeMap::from([(subject.to_string(), value)]),
        ..Default::default()
    }
}

pub fn accept_defaults() -> InterviewArgs {
    InterviewArgs {
        accept_defaults: true,
        ..Default::default()
    }
}

/// The path the app's `Answer` and `AcceptDefaults` actions take
/// (`crates/satz-studio/src/state/estate_actions.rs`): the write lock, the bytes
/// recorded, `satz_interview` on the session, the real path checked through
/// `McpChecker`. A refused call or a failed check fails the test.
pub async fn answer(
    session: &Arc<EstateSession>,
    args: InterviewArgs,
) -> (InterviewReport, Committed) {
    let _lock = session.write_lock().await;
    let snapshot = Snapshot::take(&session.main).unwrap();
    let args = serde_json::to_value(&args)
        .unwrap()
        .as_object()
        .cloned()
        .unwrap();
    let outcome = within(session.tool("satz_interview", args)).await.unwrap();
    assert!(!outcome.is_error, "{}", outcome.text);
    let report: InterviewReport = outcome.typed("satz_interview").unwrap();
    let checker = McpChecker {
        session: Arc::clone(session),
    };
    let committed = within(snapshot.verify(&checker)).await.unwrap();
    (report, committed)
}

/// The interview the smoke matrix pipes, taken the app's way: the seven typed answers
/// one call each, then the defaults. Returns the last report.
pub async fn answer_like_the_smoke_matrix(session: &Arc<EstateSession>) -> InterviewReport {
    for (subject, value) in TYPED {
        let (report, _) = answer(session, one_answer(subject, serde_json::json!(value))).await;
        assert_eq!(report.written, 1, "{subject}");
    }
    let (report, _) = answer(session, accept_defaults()).await;
    report
}

/// `satz_questions` over the session.
pub async fn questions(session: &EstateSession) -> QuestionsReport {
    let outcome = within(session.tool("satz_questions", serde_json::Map::new()))
        .await
        .unwrap();
    assert!(!outcome.is_error, "{}", outcome.text);
    outcome.typed("satz_questions").unwrap()
}

/// The line at `span` with its `// ` removed and its indentation kept — the rule of
/// the app's `uncomment_line` in `crates/satz-studio/src/state/estate_actions.rs`.
pub fn uncomment_line(text: &str, span: Span) -> Result<String, String> {
    let line = &text[span.start..span.end];
    let body = line.trim_start();
    let indent = &line[..line.len() - body.len()];
    let Some(rest) = body.strip_prefix("// ") else {
        return Err(format!("not a commented line: `{line}`"));
    };
    Ok(format!(
        "{}{indent}{rest}{}",
        &text[..span.start],
        &text[span.end..]
    ))
}

/// The app's `EnableMap`: the commented map line `scan_uses` finds, uncommented by
/// the rule above, under the delegated-write discipline — the bytes recorded, the
/// real path checked through `McpChecker`.
pub async fn enable_map(session: &Arc<EstateSession>) -> Committed {
    let _lock = session.write_lock().await;
    let snapshot = Snapshot::take(&session.main).unwrap();
    let text = std::str::from_utf8(snapshot.bytes()).unwrap().to_string();
    let cst = Cst::parse(&text).unwrap();
    let line = scan_uses(&cst)
        .into_iter()
        .find(|u| {
            u.path == MAP_PATH
                && u.gate.is_none()
                && u.as_key.is_none()
                && u.state == UseState::Commented
        })
        .expect("a commented map line");
    let new_text = uncomment_line(&text, line.span).unwrap();
    std::fs::write(&session.main, new_text).unwrap();
    let checker = McpChecker {
        session: Arc::clone(session),
    };
    within(snapshot.verify(&checker)).await.unwrap()
}

/// The model as the app's reload builds it: the questions over the session, the main
/// file parsed, the params resolved, the schema loaded, and `diagnostics` as what the
/// compile said.
pub async fn model(session: &EstateSession, diagnostics: Vec<Diagnostic>) -> EstateModel {
    let report = questions(session).await;
    let text = read(&session.main);
    let cst = Cst::parse(&text).unwrap();
    let env = session.dir.params(&session.main).unwrap();
    let registry = ResourceRegistry::load_all(&session.dir.schema_dir()).unwrap();
    EstateModel::build(
        &session.main,
        &cst,
        Ok(&registry),
        &env,
        &report,
        diagnostics,
    )
    .unwrap()
}

/// `satz --config <dir> transpile <main> --check` through the CLI runner: whether it
/// passed, and every line it printed with the version banner dropped.
pub async fn check_lines(cli: &SatzCli, main: &Path) -> (bool, Vec<CliLine>) {
    let args = vec![
        "transpile".to_string(),
        main.display().to_string(),
        "--check".to_string(),
    ];
    let (tx, mut rx) = mpsc::channel(256);
    let collect = tokio::spawn(async move {
        let mut lines = Vec::new();
        while let Some(line) = rx.recv().await {
            lines.push(line);
        }
        lines
    });
    let status = within(cli.run(&args, tx, CancellationToken::new()))
        .await
        .unwrap();
    let lines = collect
        .await
        .unwrap()
        .into_iter()
        .filter(|l| {
            let text = match l {
                CliLine::Stdout(t) | CliLine::Stderr(t) => t,
            };
            !(text.starts_with("satz v") && text.contains("(built "))
        })
        .collect();
    (status.success(), lines)
}

/// The stderr half of what `check_lines` collected, joined.
pub fn stderr_of(lines: &[CliLine]) -> String {
    lines
        .iter()
        .filter_map(|l| match l {
            CliLine::Stderr(t) => Some(t.as_str()),
            CliLine::Stdout(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// Every binding of `params { }`: name → the value's source text.
pub fn params_of(cst: &Cst) -> BTreeMap<String, String> {
    let Some(params) = cst.params() else {
        return BTreeMap::new();
    };
    cst.node(params)
        .children
        .iter()
        .filter_map(|&id| match &cst.node(id).kind {
            NodeKind::ParamEntry { name, value, .. } => Some((
                cst.slice(*name).to_string(),
                cst.slice(cst.node(*value).span).to_string(),
            )),
            _ => None,
        })
        .collect()
}

/// Every `use` line: (path, gate, active).
pub fn use_states(cst: &Cst) -> BTreeSet<(String, Option<String>, bool)> {
    scan_uses(cst)
        .into_iter()
        .map(|u| (u.path, u.gate, u.state == UseState::Active))
        .collect()
}

/// The text with the `params { … }` block cut out.
pub fn outside_params(cst: &Cst) -> String {
    let span = cst.node(cst.params().expect("a params block")).span;
    format!("{}{}", &cst.text()[..span.start], &cst.text()[span.end..])
}
