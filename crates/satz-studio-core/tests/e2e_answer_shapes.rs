//! The shape an answer is typed in, held to the packs. Decisions picks an answer's field
//! from `answer_kind` over the questions report and the model's `shapes`; a question
//! that offers no value — a param its pack declares `[]` offers none — takes the shape
//! its param is declared with, so one address typed for a list of addresses is written
//! as a list of one and never as a string that `tofu apply` refuses.
//!
//! The sweep reads every question of every pack under `vendor/satz/presets`, finds the
//! value its param is declared with in the pack's own text, and asserts that the field
//! agrees with it: a list is a list of strings, a bool a bool, a number a number. It runs over one
//! estate that uses every pack, through the installed satz and the model the app builds,
//! so a pin bump that brings a pack with a new list param fails here, not in someone's
//! apply.

#[path = "fixtures/e2e/support.rs"]
mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use satz_core::satz::{File, Value};
use satz_studio_core::cst::Cst;
use satz_studio_core::estate::EstateDir;
use satz_studio_core::model::{EstateModel, ParamKind, SourceValue, answer_kind};
use satz_studio_core::satz::reports::{QuestionKind, QuestionState, QuestionsReport};
use satz_studio_core::schema::ResourceRegistry;

/// Every pack file under `vendor/satz/presets`, parsed, by the path a `use` names it with.
fn library() -> BTreeMap<String, File> {
    let vendor = support::vendor();
    let mut stack = vec![vendor.join("presets")];
    let mut out = BTreeMap::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("satz") {
                let text = support::read(&path);
                let file = satz_core::satz::parse(&text)
                    .unwrap_or_else(|e| panic!("{}: {}", path.display(), e.msg));
                if file.is_pack {
                    out.insert(used_as(&vendor, &path), file);
                }
            }
        }
    }
    out
}

/// `presets/cis-extensions/access-approval.satz`, with forward slashes on every platform.
fn used_as(vendor: &Path, path: &Path) -> String {
    path.strip_prefix(vendor)
        .unwrap()
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// An estate that uses every pack, each after the packs whose params its own defaults
/// are built from. satz's own fold is the judge of the order: a pack goes in once
/// `estate_params` resolves with it, and a pack that never resolves fails the test by
/// name — its params are built from something no pack declares.
fn every_pack(dir: &EstateDir, main: &Path, library: &BTreeMap<String, File>) -> String {
    let first = ["presets/estate-core.satz", "presets/estate-map.satz"];
    let mut placed: Vec<String> = first.iter().map(|p| p.to_string()).collect();
    let mut waiting: Vec<String> = library
        .keys()
        .filter(|p| !first.contains(&p.as_str()))
        .cloned()
        .collect();
    let text = |uses: &[String]| {
        let lines: String = uses.iter().map(|u| format!("use \"{u}\"\n")).collect();
        format!("estate acme\n\n{lines}")
    };
    let load = dir.loader(main);
    let name = main.to_string_lossy().into_owned();
    while !waiting.is_empty() {
        let before = waiting.len();
        let mut errors = Vec::new();
        waiting.retain(|candidate| {
            let mut uses = placed.clone();
            uses.push(candidate.clone());
            match satz_core::pipeline::estate_params(&name, &text(&uses), &load) {
                Ok(_) => {
                    placed.push(candidate.clone());
                    false
                }
                Err(e) => {
                    errors.push(format!("{candidate}: {}:{}: {}", e.file, e.line, e.msg));
                    true
                }
            }
        });
        assert!(
            waiting.len() < before,
            "no order resolves these packs:\n{}",
            errors.join("\n")
        );
    }
    text(&placed)
}

/// A declared value with every reference followed to what it names.
fn followed<'a>(
    value: &'a Value,
    from: &'a str,
    library: &'a BTreeMap<String, File>,
    order: &'a [String],
) -> &'a Value {
    match value {
        Value::Ref(name) => {
            let (file, target) = declaration(name, from, library, order)
                .unwrap_or_else(|| panic!("{from}: `{name}` is declared by no pack"));
            followed(target, file, library, order)
        }
        other => other,
    }
}

/// Where `name` is declared: in the declaring file first, then in the library in the
/// estate's `use` order, the first declaration winning as the fold's does.
fn declaration<'a>(
    name: &str,
    from: &'a str,
    library: &'a BTreeMap<String, File>,
    order: &'a [String],
) -> Option<(&'a str, &'a Value)> {
    let find = |path: &'a str| {
        library[path]
            .params
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, v, _)| (path, v))
    };
    find(from).or_else(|| order.iter().find_map(|p| find(p.as_str())))
}

/// The `use` paths of an estate text, in order.
fn uses_of(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.strip_prefix("use \"")?.strip_suffix('"'))
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn every_question_of_every_pack_is_answered_in_the_shape_its_param_is_declared_with() {
    let library = library();
    let estate = support::estate_dir(None);
    let main = estate.file("C0example.satz");
    let dir = EstateDir::open(&estate.root).unwrap();
    let text = every_pack(&dir, &main, &library);
    std::fs::write(&main, &text).unwrap();
    let order = uses_of(&text);

    let cli = estate.cli().await;
    let report: QuestionsReport =
        support::within(cli.json_report(&["questions".to_string(), main.display().to_string()]))
            .await
            .unwrap();
    let env = dir.params(&main).unwrap();
    let registry = ResourceRegistry::load_all(&dir.schema_dir()).unwrap();
    let cst = Cst::parse(&text).unwrap();
    let model = EstateModel::build(&main, &cst, Ok(&registry), &env, &report, Vec::new()).unwrap();

    // the report carries every question the library declares: the sweep misses none
    let declared: BTreeSet<(String, String)> = library
        .iter()
        .flat_map(|(path, file)| {
            file.questions
                .iter()
                .map(move |q| (path.clone(), q.subject.clone()))
        })
        .collect();
    let reported: BTreeSet<(String, String)> = report
        .questions
        .iter()
        .map(|q| (q.from.clone(), q.subject.clone()))
        .collect();
    let missing: Vec<_> = declared.difference(&reported).collect();
    assert!(
        missing.is_empty(),
        "questions the report does not carry: {missing:?}"
    );

    let mut wrong = Vec::new();
    let mut lists = BTreeSet::new();
    for q in report
        .questions
        .iter()
        .filter(|q| q.kind == QuestionKind::Param)
    {
        let (file, value) =
            declaration(&q.subject, &q.from, &library, &order).unwrap_or_else(|| {
                panic!(
                    "{}: `{}` is asked and declared by no pack",
                    q.from, q.subject
                )
            });
        let want = match followed(value, file, &library, &order) {
            Value::List(items) => {
                lists.insert(q.subject.clone());
                // a list field holds strings and sends strings, as `parse_answer` reads a
                // list; a pack that declares a list of anything else needs a typed one
                if let Some(item) = items.iter().find(|i| !matches!(i, Value::Str(_))) {
                    wrong.push(format!(
                        "{} `{}`: declared a list holding {item:?}, and a list field sends strings",
                        q.from, q.subject
                    ));
                }
                ParamKind::List
            }
            Value::Bool(_) => ParamKind::Bool,
            Value::Num(_) => ParamKind::Number,
            Value::Str(_) | Value::Obj(_) => ParamKind::String,
            Value::Ref(_) => unreachable!("followed"),
        };
        // as the report has it, and as it would be with nothing offered — the path a
        // param takes the moment its default is not one satz can offer
        let mut offers_nothing = q.clone();
        offers_nothing.current = None;
        offers_nothing.default = None;
        for (how, row) in [("as reported", q), ("offering nothing", &offers_nothing)] {
            let got = answer_kind(row, Some(&model.shapes));
            if got != Some(want) {
                wrong.push(format!(
                    "{} `{}` ({how}): declared {want:?}, answered as {got:?}",
                    q.from, q.subject
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // not vacuous: the list params the library is known to declare are among the ones found
    for known in [
        "access_approval_notification_emails",
        "allowed_resource_locations",
        "admin_port_source_ranges",
        "allowed_policy_member_subjects",
    ] {
        assert!(
            lists.contains(known),
            "{known} is not a list question: {lists:?}"
        );
    }
}

/// The defect this guards, end to end on a skeleton with the access-approval pack in:
/// the question offers nothing and blocks, its field is a list because the fold carries
/// the pack's `[]`, the answer `satz_interview` receives is a JSON array, and the line
/// satz writes is a list the compile accepts.
#[tokio::test]
async fn one_address_for_a_list_param_is_written_as_a_list_of_one() {
    const SUBJECT: &str = "access_approval_notification_emails";
    let estate = support::estate_dir(None);
    let main: PathBuf = estate.create_skeleton("C0example.satz").await;
    let skeleton = support::read(&main);
    let commented =
        "// use \"presets/cis-extensions/access-approval.satz\" when cis_access_approval";
    assert!(
        skeleton.contains(commented),
        "the skeleton carries the pack's line"
    );
    std::fs::write(
        &main,
        skeleton.replace(
            commented,
            "use \"presets/cis-extensions/access-approval.satz\"",
        ),
    )
    .unwrap();
    let session = estate.open("C0example.satz").await;

    let env = session.dir.params(&main).unwrap();
    assert_eq!(
        env.get(SUBJECT),
        Some(&serde_yaml::Value::Sequence(Vec::new())),
        "the fold carries the pack's declaration"
    );
    let model = support::model(&session, Vec::new()).await;
    let report = support::questions(&session).await;
    let row = report
        .questions
        .iter()
        .find(|q| q.subject == SUBJECT)
        .expect("the pack asks it");
    assert_eq!(row.state, QuestionState::Unanswered);
    assert!(row.blocking, "{row:?}");
    assert_eq!(row.offered(), None, "satz offers no empty list");
    assert_eq!(answer_kind(row, Some(&model.shapes)), Some(ParamKind::List));

    // what a list field sends for one chip
    let value = serde_json::json!(["security@example.com"]);
    assert!(value.is_array());
    let (answered, committed) =
        support::answer(&session, support::one_answer(SUBJECT, value.clone())).await;
    assert_eq!(answered.written, 1);
    assert_eq!(committed.path, main);

    let after = support::model(&session, Vec::new()).await;
    let param = after
        .params
        .iter()
        .find(|p| p.name == SUBJECT)
        .expect("the answer is a param of the estate");
    assert_eq!(param.kind, ParamKind::List);
    assert!(
        matches!(&param.value, SourceValue::List(items) if items.len() == 1),
        "{:?}",
        param.value
    );
    let row = support::questions(&session)
        .await
        .questions
        .into_iter()
        .find(|q| q.subject == SUBJECT)
        .unwrap();
    assert_eq!(row.state, QuestionState::Answered);
    assert_eq!(row.current, Some(value));
    assert!(estate.temp_files().is_empty(), "{:?}", estate.temp_files());
}
