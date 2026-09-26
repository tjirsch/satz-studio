//! The param rows of the showcase estate, joined with the questions the installed satz
//! reports for it; the gates of satz's pack report, which are the Packs view's instead;
//! the kinds an answer is read in.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use satz_core::pipeline::Env;
use satz_studio_core::cst::Cst;
use satz_studio_core::estate::EstateDir;
use satz_studio_core::model::{EditMode, EstateModel, ParamKind, ParamRow, SourceValue};
use satz_studio_core::satz::reports::{
    PacksReport, QuestionKind, QuestionState, QuestionsReport, QuestionsSummary,
};
use satz_studio_core::satz::{SatzBinary, SatzCli};
use satz_studio_core::schema::ResourceRegistry;

const TIME_BOX: Duration = Duration::from_secs(60);

fn fixture() -> EstateDir {
    EstateDir::open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/smoke"))
        .unwrap()
}

async fn json_report<T: serde::de::DeserializeOwned>(
    estate: &EstateDir,
    command: &str,
    name: &str,
) -> T {
    let bin = SatzBinary::locate(None).await.unwrap();
    let cli = SatzCli::new(bin, estate.dir.canonicalize().unwrap());
    let args = [command.to_string(), name.to_string()];
    tokio::time::timeout(TIME_BOX, cli.json_report(&args))
        .await
        .unwrap()
        .unwrap()
}

fn row<'a>(rows: &'a [ParamRow], name: &str) -> &'a ParamRow {
    rows.iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no param row `{name}`"))
}

#[tokio::test]
async fn showcase_params_carry_their_questions_and_leave_the_gates_out() {
    let estate = fixture();
    let main = estate.yaml_dir().join("showcase.satz");
    let report: QuestionsReport = json_report(&estate, "questions", "showcase.satz").await;
    let packs: PacksReport = json_report(&estate, "packs", "showcase.satz").await;
    let text = std::fs::read_to_string(&main).unwrap();
    let cst = Cst::parse(&text).unwrap();
    let env = estate.params(&main).unwrap();
    let registry = ResourceRegistry::load_all(&estate.schema_dir()).unwrap();
    let m = EstateModel::build(
        &main,
        &cst,
        Ok(&registry),
        &env,
        &report,
        &packs,
        Vec::new(),
    )
    .unwrap();

    let names: Vec<&str> = m.params.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "customer_organization_id",
            "customer_id",
            "customer_domain",
            "customer_shortname",
            "infra_project_name",
            "billing_account_infra",
            "default_region",
            "audit_retention_days",
            "pack_bucket_location",
            "pack_bucket_adopted",
            "team_folder_name",
            "archive_project_folder",
            "infra_project_services",
            "event_topics",
        ],
        "the options of group_model and optional_extras are choices, answered as one"
    );

    let region = row(&m.params, "default_region");
    let q = region
        .question
        .as_ref()
        .expect("default_region has a question");
    assert_eq!(q.kind, QuestionKind::Param);
    assert_eq!(q.state, QuestionState::Answered);
    assert!(
        q.recommend
            .as_deref()
            .is_some_and(|r| r.contains("europe-west3")),
        "{:?}",
        q.recommend
    );
    assert!(!region.one_way_door, "state_surgery + low");
    assert_eq!(region.kind, ParamKind::String);
    assert_eq!(region.mode, EditMode::Value);

    let shortname = row(&m.params, "customer_shortname");
    assert!(shortname.one_way_door, "reversal = recreate");
    assert_eq!(
        shortname.value,
        SourceValue::Str {
            raw: "corp".to_string(),
            parts: vec![satz_studio_core::model::StrPart::Lit("corp".to_string())]
        }
    );

    let retention = row(&m.params, "audit_retention_days");
    assert_eq!(retention.kind, ParamKind::Number);
    assert_eq!(retention.value, SourceValue::Num("400".to_string()));
    assert!(retention.question.is_none());
    assert!(!retention.one_way_door);

    assert!(row(&m.params, "customer_id").question.is_none());
    // every row sits on the line that binds it, wherever the params block stands
    let lines: Vec<&str> = text.lines().collect();
    for r in &m.params {
        let at = lines[r.line as usize - 1];
        assert_eq!(
            at.split_once('=').map(|(name, _)| name.trim()),
            Some(r.name.as_str()),
            "{}: line {} is `{at}`",
            r.name,
            r.line
        );
    }

    // the showcase's own switch is a file the pack graph does not know: its line is
    // the estate's, and its gate the option of a choice, answered as one
    assert!(
        m.packs
            .unmanaged
            .iter()
            .any(|u| u.path == "showcase-optional.satz"),
        "{:?}",
        m.packs.unmanaged
    );
    assert!(m.params.iter().all(|r| r.name != "want_optional"));
    // a question whose `empty` says what "" means: its "" is an answer
    let team = row(&m.params, "team_folder_name");
    let q = team
        .question
        .as_ref()
        .expect("team_folder_name has a question");
    assert_eq!(q.state, QuestionState::Answered);
    assert_eq!(q.empty.as_deref(), Some("no team folder"));
    assert!(
        m.packs
            .packs
            .iter()
            .filter_map(|p| p.gate.as_deref())
            .all(|g| m.params.iter().all(|r| r.name != g)),
        "a gate of the pack graph is no param row"
    );
}

#[test]
fn a_reference_takes_the_shape_of_what_it_resolves_to() {
    let text = "estate x\n\nparams {\n  a = true\n  b = a\n  c = \"{a}\"\n  d = [\"x\", \"y\"]\n  e = 3\n  f = e\n  g = unbound\n}\n";
    let cst = Cst::parse(text).unwrap();
    let mut env = Env::new();
    env.insert("a".to_string(), serde_yaml::Value::Bool(true));
    env.insert("b".to_string(), serde_yaml::Value::Bool(true));
    env.insert(
        "c".to_string(),
        serde_yaml::Value::String("true".to_string()),
    );
    env.insert("e".to_string(), serde_yaml::from_str("3").unwrap());
    env.insert("f".to_string(), serde_yaml::from_str("3").unwrap());
    let main = Path::new("inline.satz");
    let report = QuestionsReport {
        estate: main.display().to_string(),
        questions: Vec::new(),
        summary: QuestionsSummary::default(),
    };
    let packs = PacksReport {
        estate: main.display().to_string(),
        note: None,
        packs: Vec::new(),
        unmanaged: Vec::new(),
        interfaces: Vec::new(),
        findings: Vec::new(),
    };
    let m = EstateModel::build(
        main,
        &cst,
        Err(Path::new("/nowhere")),
        &env,
        &report,
        &packs,
        Vec::new(),
    )
    .unwrap();
    let kinds: BTreeMap<&str, (ParamKind, EditMode)> = m
        .params
        .iter()
        .map(|r| (r.name.as_str(), (r.kind, r.mode)))
        .collect();
    assert_eq!(kinds["a"], (ParamKind::Bool, EditMode::Value));
    assert_eq!(kinds["b"], (ParamKind::Bool, EditMode::Source));
    assert_eq!(kinds["c"], (ParamKind::String, EditMode::Source));
    assert_eq!(kinds["d"], (ParamKind::List, EditMode::Value));
    assert_eq!(kinds["e"], (ParamKind::Number, EditMode::Value));
    assert_eq!(kinds["f"], (ParamKind::Number, EditMode::Source));
    assert_eq!(kinds["g"], (ParamKind::String, EditMode::Source));
    assert_eq!(
        row(&m.params, "g").value,
        SourceValue::Ref {
            param: "unbound".to_string(),
            resolved: None
        }
    );
    assert_eq!(
        row(&m.params, "b").value,
        SourceValue::Ref {
            param: "a".to_string(),
            resolved: Some(serde_json::json!(true))
        }
    );
}
