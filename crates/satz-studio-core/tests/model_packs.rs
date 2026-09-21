//! The pack rows over the skeleton `satz interview --create` writes, with the map line
//! commented and then on; an estate whose map gate has no line (`Absent`); a line that is
//! active while its gate is false (an `Info`). The questions come from the installed satz.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use satz_studio_core::cst::Cst;
use satz_studio_core::diag::Severity;
use satz_studio_core::estate::EstateDir;
use satz_studio_core::model::{
    Choice, EstateModel, LineState, MAP_PATH, PackDecls, PackRow, PackRowKind, ResourceKind,
};
use satz_studio_core::satz::reports::{QuestionKind, QuestionState, QuestionsReport};
use satz_studio_core::satz::{SatzBinary, SatzCli};
use satz_studio_core::schema::ResourceRegistry;
use tempfile::TempDir;

const TIME_BOX: Duration = Duration::from_secs(60);

fn vendor() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/satz")
        .canonicalize()
        .expect("vendor/satz")
}

/// A temporary estate directory whose config points into `vendor/satz`, and the
/// skeleton `satz interview --create` writes into it as `yaml/new.satz`.
fn skeleton() -> (TempDir, PathBuf) {
    let vendor = vendor();
    let tmp = tempfile::tempdir().unwrap();
    let yaml = tmp.path().join("yaml");
    fs::create_dir_all(&yaml).unwrap();
    let config = format!(
        "yaml_dir = {yaml:?}\nhcl_dir = \"hcl\"\ninclude_dirs = [\".\", {yaml:?}, {vendor:?}]\npresets_dir = {presets:?}\nschema_dir = {schemas:?}\ntf_tool = \"tofu\"\nprovider_version = \"7.14.1\"\n",
        presets = vendor.join("presets"),
        schemas = vendor.join("tests/schemas"),
    );
    fs::write(tmp.path().join("config.toml"), config).unwrap();
    let estate = yaml.join("new.satz");
    let bin = satz_binary();
    let out = Command::new(bin)
        .arg("--config")
        .arg(tmp.path())
        .arg("interview")
        .arg(&estate)
        .arg("--create")
        .stdin(Stdio::null())
        .output()
        .expect("run satz");
    assert!(
        out.status.success(),
        "satz interview --create failed ({}):\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    (tmp, estate)
}

fn satz_binary() -> PathBuf {
    if let Ok(p) = which::which("satz") {
        return p;
    }
    let local = dirs::home_dir()
        .expect("a home directory")
        .join(".local/bin/satz");
    assert!(
        local.exists(),
        "satz is neither on PATH nor at {}",
        local.display()
    );
    local
}

/// `text` written as `<stem>.satz` beside the skeleton, its `estate` line renamed.
fn variant(tmp: &TempDir, stem: &str, text: &str) -> PathBuf {
    let path = tmp.path().join("yaml").join(format!("{stem}.satz"));
    fs::write(
        &path,
        text.replacen("estate new", &format!("estate {stem}"), 1),
    )
    .unwrap();
    path
}

async fn questions(config_dir: &Path, estate: &Path) -> QuestionsReport {
    let bin = SatzBinary::locate(None).await.unwrap();
    let cli = SatzCli::new(bin, config_dir.to_path_buf());
    let args = ["questions".to_string(), estate.display().to_string()];
    tokio::time::timeout(TIME_BOX, cli.json_report(&args))
        .await
        .unwrap()
        .unwrap()
}

async fn model(config_dir: &Path, estate: &Path) -> EstateModel {
    let dir = EstateDir::open(config_dir).unwrap();
    let report = questions(config_dir, estate).await;
    let text = fs::read_to_string(estate).unwrap();
    let cst = Cst::parse(&text).unwrap();
    let env = dir.params(estate).unwrap();
    let registry = ResourceRegistry::load_all(&dir.schema_dir()).unwrap();
    let decls = PackDecls::read(estate, &cst, &dir.loader(estate));
    EstateModel::build(
        estate,
        &cst,
        Ok(&registry),
        &env,
        &report,
        &decls,
        Vec::new(),
    )
    .unwrap()
}

fn by_gate<'a>(rows: &'a [PackRow], gate: &str) -> Vec<&'a PackRow> {
    rows.iter()
        .filter(|r| r.gate.as_deref() == Some(gate))
        .collect()
}

fn one<'a>(rows: &'a [PackRow], gate: &str) -> &'a PackRow {
    let found = by_gate(rows, gate);
    assert_eq!(found.len(), 1, "gate `{gate}`: {found:?}");
    found[0]
}

#[tokio::test]
async fn the_skeleton_is_the_map_row_then_every_gated_line_off() {
    let (tmp, estate) = skeleton();
    let m = model(tmp.path(), &estate).await;
    let text = fs::read_to_string(&estate).unwrap();

    let map = &m.packs[0];
    assert_eq!(map.kind, PackRowKind::Map);
    assert_eq!(map.state, LineState::Off);
    assert_eq!(map.path.as_deref(), Some(MAP_PATH));
    assert_eq!(map.gate, None);
    assert_eq!(map.choice, Choice::Line);
    assert_eq!(map.question, None);
    assert!(
        map.phase
            .as_deref()
            .is_some_and(|p| p.starts_with("once the estate runs as the service account")),
        "{:?}",
        map.phase
    );
    let map_line = text
        .lines()
        .position(|l| l == "// use \"presets/estate-map.satz\"")
        .unwrap() as u32
        + 1;
    assert_eq!(map.line, Some(map_line));

    let gated = text.lines().filter(|l| l.contains("\" when ")).count();
    // every use line that is neither gated nor the map: the skeleton's own
    // `use "presets/estate-core.satz"`, which no question decides
    let plain = text
        .lines()
        .map(str::trim_start)
        .filter(|l| {
            (l.starts_with("use \"") || l.starts_with("// use \""))
                && !l.contains("\" when ")
                && !l.contains(MAP_PATH)
        })
        .count();
    assert!(plain > 0, "the skeleton runs a pack no question gates");
    assert_eq!(
        m.packs.len(),
        1 + gated + plain,
        "the map row, then one row per line — gated or not, no Absent row"
    );
    for row in m.packs[1..].iter().filter(|r| r.kind == PackRowKind::Plain) {
        assert_eq!(row.gate, None, "{row:?}");
        assert_eq!(row.question, None, "{row:?}");
        assert_eq!(row.choice, Choice::Line, "the line is the whole decision");
        assert!(row.path.is_some() && row.line.is_some(), "{row:?}");
    }
    for row in m.packs[1..]
        .iter()
        .filter(|r| r.kind == PackRowKind::Choice)
    {
        assert_eq!(row.state, LineState::Off, "{row:?}");
        assert!(
            row.gate.is_some() && row.path.is_some() && row.line.is_some(),
            "{row:?}"
        );
        assert_eq!(
            row.question, None,
            "the map is commented, so satz asks none of its questions yet: {row:?}"
        );
        assert!(
            matches!(
                row.choice,
                Choice::Bool {
                    current: None,
                    default: None
                }
            ),
            "{row:?}"
        );
    }
    // the map row comes first by design; the rest follow the file
    assert!(
        m.packs[1..].windows(2).all(|w| w[0].line < w[1].line),
        "document order"
    );

    // every line of the skeleton is its own row: the runner and the grant that goes
    // with it are two, each with its own gate
    let runner = one(&m.packs, "use_verification_runner");
    assert!(
        runner
            .path
            .as_deref()
            .unwrap()
            .ends_with("verification-runner.satz"),
        "{runner:?}"
    );
    let grant = one(&m.packs, "use_verification_runner_grant");
    assert!(
        grant
            .path
            .as_deref()
            .unwrap()
            .ends_with("verification-runner-grant.satz"),
        "{grant:?}"
    );

    let logsink = one(&m.packs, "use_audit_logsink");
    assert!(
        logsink
            .phase
            .as_deref()
            .is_some_and(|p| p.contains("the audit archive")),
        "{:?}",
        logsink.phase
    );
    let alerts = one(&m.packs, "use_central_alerts");
    assert!(
        alerts
            .phase
            .as_deref()
            .is_some_and(|p| p.starts_with("once the archive exists")),
        "{:?}",
        alerts.phase
    );
    let folder = m
        .outline
        .iter()
        .find(|n| n.key == "google_folder")
        .expect("the folder map");
    let infra = &folder.children[0];
    assert_eq!(infra.kind, ResourceKind::Resource);
    assert!(infra.uses.is_empty(), "{:?}", infra.uses);
    let top: Vec<u32> = m.uses.iter().map(|u| u.line).collect();
    for line in [logsink.line.unwrap(), alerts.line.unwrap()] {
        assert!(
            top.contains(&line),
            "the two logging lines stand at the top level, outside the infra folder: {top:?}"
        );
    }

    assert!(m.diagnostics.is_empty(), "{:?}", m.diagnostics);
    assert!(m.params.is_empty(), "the skeleton binds nothing yet");
}

#[tokio::test]
async fn with_the_map_on_every_row_has_its_question_and_the_oneof_is_two_options() {
    let (tmp, estate) = skeleton();
    let text = fs::read_to_string(&estate).unwrap();
    let on = variant(
        &tmp,
        "on",
        &text.replacen(
            "// use \"presets/estate-map.satz\"",
            "use \"presets/estate-map.satz\"",
            1,
        ),
    );
    let m = model(tmp.path(), &on).await;

    assert_eq!(m.packs[0].kind, PackRowKind::Map);
    assert_eq!(m.packs[0].state, LineState::On);
    assert!(
        m.packs.iter().all(|r| r.state != LineState::Absent),
        "the skeleton carries a line for every choice of the map"
    );

    let s1 = one(&m.packs, "security_model_s1");
    assert_eq!(
        s1.choice,
        Choice::OneofOption {
            group: "security_model".to_string(),
            selected: true
        }
    );
    let q = s1.question.as_ref().expect("the oneof question");
    assert_eq!(q.kind, QuestionKind::Oneof);
    assert_eq!(q.subject, "security_model");
    let s2 = one(&m.packs, "security_model_s2");
    assert_eq!(
        s2.choice,
        Choice::OneofOption {
            group: "security_model".to_string(),
            selected: false
        }
    );
    assert_eq!(s2.question, s1.question);

    let budget = one(&m.packs, "use_budget");
    assert_eq!(budget.state, LineState::Off);
    assert_eq!(
        budget.choice,
        Choice::Bool {
            current: Some(false),
            default: Some(false)
        }
    );
    let q = budget.question.as_ref().expect("use_budget's question");
    assert_eq!(q.state, QuestionState::Unanswered);
    assert_eq!(q.pack, "estate_map");

    let logsink = one(&m.packs, "use_audit_logsink");
    assert_eq!(
        logsink.choice,
        Choice::Bool {
            current: Some(true),
            default: Some(true)
        }
    );

    let notifications = one(&m.packs, "use_scc_notifications");
    assert_eq!(
        notifications.question.as_ref().map(|q| q.state),
        Some(QuestionState::NotApplicable)
    );
    assert_eq!(
        notifications.choice,
        Choice::Bool {
            current: Some(false),
            default: None
        }
    );

    let ssh = one(&m.packs, "cis_block_project_ssh_keys");
    assert_eq!(ssh.question, None, "the CIS baseline is still commented");
    assert!(m.diagnostics.is_empty(), "{:?}", m.diagnostics);
}

#[tokio::test]
async fn a_map_gate_without_a_line_is_absent_and_not_a_param_row() {
    let (tmp, estate) = skeleton();
    let text = fs::read_to_string(&estate).unwrap();
    let budget_line = "// use \"presets/organization-budget.satz\" when use_budget\n";
    assert!(text.contains(budget_line));
    let absent = variant(
        &tmp,
        "absent",
        &text
            .replacen(
                "// use \"presets/estate-map.satz\"",
                "use \"presets/estate-map.satz\"",
                1,
            )
            .replacen(budget_line, "", 1)
            .replacen("params {\n}", "params {\n  use_budget = true\n}", 1),
    );
    let m = model(tmp.path(), &absent).await;

    let absent_rows: Vec<&PackRow> = m
        .packs
        .iter()
        .filter(|r| r.state == LineState::Absent)
        .collect();
    assert_eq!(absent_rows.len(), 1, "{absent_rows:?}");
    let row = absent_rows[0];
    assert_eq!(row.kind, PackRowKind::Choice);
    assert_eq!(row.gate.as_deref(), Some("use_budget"));
    assert_eq!(row.path, None);
    assert_eq!(row.line, None);
    assert_eq!(row.phase, None);
    assert_eq!(
        row.choice,
        Choice::Bool {
            current: Some(true),
            default: None
        }
    );
    assert_eq!(
        row.question.as_ref().map(|q| q.state),
        Some(QuestionState::Answered)
    );
    assert_eq!(
        m.packs.last().unwrap().gate.as_deref(),
        Some("use_budget"),
        "Absent rows come last"
    );
    assert!(
        m.params.iter().all(|p| p.name != "use_budget"),
        "a pack row's gate is not a param row: {:?}",
        m.params.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn an_active_line_whose_gate_is_false_is_a_note() {
    let (tmp, estate) = skeleton();
    let text = fs::read_to_string(&estate).unwrap();
    let noted = variant(
        &tmp,
        "noted",
        &text
            .replacen(
                "// use \"presets/estate-map.satz\"",
                "use \"presets/estate-map.satz\"",
                1,
            )
            .replacen(
                "// use \"presets/organization-budget.satz\" when use_budget",
                "use \"presets/organization-budget.satz\" when use_budget",
                1,
            )
            .replacen("params {\n}", "params {\n  use_budget = false\n}", 1),
    );
    let m = model(tmp.path(), &noted).await;

    let budget = one(&m.packs, "use_budget");
    assert_eq!(budget.state, LineState::On);
    assert_eq!(
        budget.choice,
        Choice::Bool {
            current: Some(false),
            default: None
        }
    );
    assert_eq!(m.diagnostics.len(), 1, "{:?}", m.diagnostics);
    let note = &m.diagnostics[0];
    assert_eq!(note.severity, Severity::Info);
    assert_eq!(note.file.as_deref(), Some(noted.as_path()));
    assert_eq!(note.line, budget.line);
    assert!(note.message.contains("use_budget"), "{}", note.message);
    assert!(
        note.message.starts_with("line active, gate false"),
        "{}",
        note.message
    );
}

#[tokio::test]
async fn showcase_has_one_gated_line_that_is_active_while_false() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/smoke")
        .canonicalize()
        .unwrap();
    let estate = EstateDir::open(&dir).unwrap();
    let main = estate.yaml_dir().join("showcase.satz");
    let m = model(&dir, &main).await;

    assert_eq!(m.packs[0].kind, PackRowKind::Map);
    assert_eq!(
        m.packs[0].state,
        LineState::Absent,
        "showcase does not use the map"
    );
    assert_eq!(m.packs[0].path.as_deref(), Some(MAP_PATH));
    // Three use lines, and only one of them is gated. The two that are not were in the
    // estate and in no row until 2026-09-17 — the same silence that hid satz's CIS
    // baseline while it carried no `when`.
    assert_eq!(m.packs.len(), 4, "{:?}", m.packs);
    let plain: Vec<_> = m
        .packs
        .iter()
        .filter(|r| r.kind == PackRowKind::Plain)
        .collect();
    assert_eq!(
        plain
            .iter()
            .map(|r| r.path.as_deref().unwrap_or_default())
            .collect::<Vec<_>>(),
        ["showcase-pack.satz", "showcase-policies.satz"]
    );
    for row in &plain {
        assert_eq!(row.state, LineState::On, "{row:?}");
        assert_eq!(row.gate, None, "{row:?}");
        assert_eq!(row.question, None, "{row:?}");
        assert_eq!(row.choice, Choice::Line, "{row:?}");
    }
    let optional = one(&m.packs, "want_optional");
    assert_eq!(optional.state, LineState::On);
    assert_eq!(optional.path.as_deref(), Some("showcase-optional.satz"));
    assert_eq!(optional.question, None);
    assert_eq!(
        optional.choice,
        Choice::Bool {
            current: Some(false),
            default: None
        }
    );
    assert_eq!(m.diagnostics.len(), 1, "{:?}", m.diagnostics);
    assert_eq!(m.diagnostics[0].severity, Severity::Info);
    assert_eq!(m.diagnostics[0].line, optional.line);
}
