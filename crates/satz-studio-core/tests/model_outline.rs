//! The outline of satz's smoke and showcase estates, classified against the schema
//! fixture: configuration, the folder → project → bucket hierarchy, nested blocks,
//! member grants, the decoded values, and what is locked. Offline: the questions
//! report is empty, which the outline does not read.

use std::path::{Path, PathBuf};

use satz_core::pipeline::Env;
use satz_studio_core::cst::{Cst, UseState};
use satz_studio_core::estate::EstateDir;
use satz_studio_core::model::{
    AttrRow, EditMode, EstateModel, PackDecls, ResourceKind, ResourceNode, SchemaStatus,
    SourceValue, StrPart,
};
use satz_studio_core::satz::reports::{QuestionsReport, QuestionsSummary};
use satz_studio_core::schema::{AttrType, ResourceRegistry};

fn fixture() -> EstateDir {
    EstateDir::open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/smoke"))
        .unwrap()
}

fn no_questions(main: &Path) -> QuestionsReport {
    QuestionsReport {
        estate: main.display().to_string(),
        questions: Vec::new(),
        summary: QuestionsSummary::default(),
    }
}

fn model_of(name: &str) -> EstateModel {
    let estate = fixture();
    let main = estate.yaml_dir().join(name);
    let text = std::fs::read_to_string(&main).unwrap();
    let cst = Cst::parse(&text).unwrap();
    let env = estate.params(&main).unwrap();
    let registry = ResourceRegistry::load_all(&estate.schema_dir()).unwrap();
    let decls = PackDecls::read(&main, &cst, &estate.loader(&main));
    EstateModel::build(
        &main,
        &cst,
        Ok(&registry),
        &env,
        &no_questions(&main),
        &decls,
        Vec::new(),
    )
    .unwrap()
}

fn inline(text: &str, env: &Env) -> EstateModel {
    let cst = Cst::parse(text).unwrap();
    let registry = ResourceRegistry::load_all(&fixture().schema_dir()).unwrap();
    let main = Path::new("inline.satz");
    EstateModel::build(
        main,
        &cst,
        Ok(&registry),
        env,
        &no_questions(main),
        &PackDecls::default(),
        Vec::new(),
    )
    .unwrap()
}

fn child<'a>(nodes: &'a [ResourceNode], key: &str) -> &'a ResourceNode {
    nodes
        .iter()
        .find(|n| n.key == key)
        .unwrap_or_else(|| panic!("no node `{key}` among {:?}", keys(nodes)))
}

fn keys(nodes: &[ResourceNode]) -> Vec<&str> {
    nodes.iter().map(|n| n.key.as_str()).collect()
}

fn row<'a>(node: &'a ResourceNode, key: &str) -> &'a AttrRow {
    node.attrs
        .iter()
        .find(|a| a.key == key)
        .unwrap_or_else(|| panic!("no row `{key}` in `{}`", node.key))
}

fn param(name: &str, resolved: &str) -> StrPart {
    StrPart::Param {
        name: name.to_string(),
        resolved: Some(serde_json::json!(resolved)),
    }
}

#[test]
fn smoke_is_configuration_then_maps_of_resources() {
    let m = model_of("smoke.satz");
    assert_eq!(
        m.schema,
        SchemaStatus::Loaded {
            providers: vec!["registry.opentofu.org/hashicorp/google".to_string()],
            resources: 45
        }
    );
    assert_eq!(
        keys(&m.outline),
        [
            "terraform",
            "providers",
            "google_essential_contacts_contact",
            "google_cloud_identity_group",
            "google_organization_iam_member",
            "google_folder",
            "google_billing_account_iam_member",
        ]
    );
    // The one use line of smoke that sits outside every block: since satz v0.64.0
    // (ADR 0028) the CIS baseline declares its own `google_org_policy_policy` and is
    // `use`d bare, so the estate has no `google_org_policy_policy` block of its own —
    // the keys above say so — and the baseline is an estate-level use. Every other use
    // line still sits in the block it fills.
    assert_eq!(m.uses.len(), 1, "{:?}", m.uses);
    assert_eq!(m.uses[0].path, "presets/cis/CIS-GCP-Foundation-4.0.satz");
    assert_eq!(m.uses[0].state, UseState::Active);
    assert!(m.uses[0].as_key.is_none(), "{:?}", m.uses[0]);

    let terraform = child(&m.outline, "terraform");
    assert_eq!(terraform.kind, ResourceKind::Config);
    let backend = child(&terraform.children, "backend");
    assert_eq!(backend.kind, ResourceKind::Config);
    let local = child(&backend.children, "local");
    assert_eq!(local.kind, ResourceKind::Config);
    let path = row(local, "path");
    assert!(path.editable);
    assert_eq!(path.typed, AttrType::Unknown);
    assert_eq!(path.mode, EditMode::Value);

    let providers = child(&m.outline, "providers");
    let beta = child(&providers.children, "google-beta");
    assert_eq!(beta.kind, ResourceKind::Config);
    assert_eq!(
        beta.key_parts,
        vec![StrPart::Lit("google-beta".to_string())]
    );
    let project = row(beta, "project");
    assert_eq!(
        project.value,
        SourceValue::Ref {
            param: "infra_project_name".to_string(),
            resolved: Some(serde_json::json!("corp-infra-001"))
        }
    );
    assert_eq!(project.mode, EditMode::Source);
    assert!(project.editable);
}

#[test]
fn smoke_nests_the_hierarchy_and_types_the_rows() {
    let m = model_of("smoke.satz");
    let folders = child(&m.outline, "google_folder");
    assert_eq!(folders.kind, ResourceKind::ResourceMap);
    assert_eq!(keys(&folders.children), ["infra_folder", "logging"]);

    let infra_folder = child(&folders.children, "infra_folder");
    assert_eq!(infra_folder.kind, ResourceKind::Resource);
    assert_eq!(infra_folder.tf_type.as_deref(), Some("google_folder"));
    assert_eq!(infra_folder.name(), Some("infra_folder"));
    assert_eq!(infra_folder.label, None);
    assert!(
        infra_folder.missing_required.is_empty(),
        "parent and display_name are derived: {:?}",
        infra_folder.missing_required
    );

    let projects = child(&infra_folder.children, "google_project");
    assert_eq!(projects.kind, ResourceKind::ResourceMap);
    let infra = child(&projects.children, "infra");
    assert_eq!(infra.kind, ResourceKind::Resource);
    assert_eq!(infra.tf_type.as_deref(), Some("google_project"));
    let project_id = row(infra, "project_id");
    assert!(project_id.required);
    assert_eq!(project_id.typed, AttrType::String);
    assert_eq!(project_id.mode, EditMode::Source);
    let services = row(infra, "project_service");
    assert_eq!(
        services.typed,
        AttrType::Unknown,
        "satz's own key, not the provider's"
    );
    assert!(services.editable);
    assert_eq!(services.mode, EditMode::Value);
    assert!(
        infra.missing_required.is_empty(),
        "{:?}",
        infra.missing_required
    );

    let buckets = child(&infra.children, "google_storage_bucket");
    let state = child(&buckets.children, "state");
    assert_eq!(state.kind, ResourceKind::Resource);
    let ubla = row(state, "uniform_bucket_level_access");
    assert_eq!(ubla.typed, AttrType::Bool);
    assert_eq!(ubla.value, SourceValue::Bool(true));
    assert!(ubla.editable, "optional + computed is editable");
    let location = row(state, "location");
    assert_eq!(
        location.value,
        SourceValue::Str {
            raw: "EU".to_string(),
            parts: vec![StrPart::Lit("EU".to_string())]
        }
    );
    assert_eq!(location.mode, EditMode::Value);
    let versioning = child(&state.children, "versioning");
    assert_eq!(versioning.kind, ResourceKind::NestedBlock);
    assert_eq!(versioning.tf_type, None);
    let enabled = row(versioning, "enabled");
    assert_eq!(enabled.typed, AttrType::Bool);
    assert!(enabled.editable);
    assert!(
        state.missing_required.is_empty(),
        "{:?}",
        state.missing_required
    );

    let logging = child(&folders.children, "logging");
    assert_eq!(logging.kind, ResourceKind::Resource);
    let paths: Vec<&str> = logging.uses.iter().map(|u| u.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "presets/monitoring/organization-audit-logsink.satz",
            "presets/monitoring/organization-cis-log-alerts-central.satz"
        ]
    );
    assert!(infra_folder.uses.is_empty());
}

#[test]
fn smoke_reads_grants_as_member_nodes() {
    let m = model_of("smoke.satz");
    let groups = child(&m.outline, "google_cloud_identity_group");
    let users = child(&groups.children, "{svc_iac_users_group}");
    assert_eq!(users.kind, ResourceKind::Resource);
    assert_eq!(users.name(), Some("{svc_iac_users_group}"));
    assert_eq!(
        users.key_parts,
        vec![param("svc_iac_users_group", "svc-iac-001-users")]
    );
    assert!(
        users.missing_required.is_empty(),
        "{:?}",
        users.missing_required
    );
    let member = row(users, "member");
    assert_eq!(member.typed, AttrType::Unknown);
    assert_eq!(member.mode, EditMode::Source, "a reference inside the list");
    assert_eq!(row(users, "display_name").typed, AttrType::String);

    let org = child(&m.outline, "google_organization_iam_member");
    assert_eq!(org.kind, ResourceKind::ResourceMap);
    assert!(org.attrs.is_empty());
    assert_eq!(org.children.len(), 2);
    let sa = &org.children[0];
    assert_eq!(sa.kind, ResourceKind::MemberGrant);
    assert_eq!(
        sa.tf_type.as_deref(),
        Some("google_organization_iam_member")
    );
    assert!(
        sa.key.starts_with("serviceAccount:{svc_iac_account}@"),
        "{}",
        sa.key
    );
    assert_eq!(
        sa.key_parts,
        vec![
            StrPart::Lit("serviceAccount:".to_string()),
            param("svc_iac_account", "svc-iac-001"),
            StrPart::Lit("@".to_string()),
            param("infra_project_name", "corp-infra-001"),
            StrPart::Lit(".iam.gserviceaccount.com".to_string()),
        ]
    );
    assert_eq!(sa.attrs.len(), 1);
    let roles = &sa.attrs[0];
    assert_eq!(roles.key, sa.key);
    assert!(roles.editable);
    assert_eq!(roles.mode, EditMode::Value, "plain role strings");
    match &roles.value {
        SourceValue::List(items) => assert_eq!(items.len(), 17),
        other => panic!("roles are a list, got {other:?}"),
    }

    let billing = child(&m.outline, "google_billing_account_iam_member");
    let id = row(billing, "billing_account_id");
    assert!(id.required);
    assert_eq!(id.typed, AttrType::String);
    assert_eq!(id.mode, EditMode::Source);
    assert_eq!(billing.children.len(), 1);
    assert_eq!(billing.children[0].kind, ResourceKind::MemberGrant);
}

#[test]
fn showcase_decodes_references_locks_import_ids_and_keeps_statements_out() {
    let m = model_of("showcase.satz");
    assert_eq!(
        keys(&m.outline),
        [
            "terraform",
            "providers",
            "google_cloud_identity_group",
            "google_organization_iam_member",
            "google_folder",
        ],
        "claim, question, action, suppress and hcl are not blocks"
    );
    let top: Vec<(&str, Option<&str>, UseState)> = m
        .uses
        .iter()
        .map(|u| (u.path.as_str(), u.gate.as_deref(), u.state))
        .collect();
    assert_eq!(
        top,
        [
            ("showcase-pack.satz", None, UseState::Active),
            ("showcase-policies.satz", None, UseState::Active),
            (
                "showcase-optional.satz",
                Some("want_optional"),
                UseState::Active
            ),
        ]
    );

    let groups = child(&m.outline, "google_cloud_identity_group");
    let auditors = child(&groups.children, "gcp-auditors");
    let import_id = row(auditors, "import-id");
    assert!(!import_id.editable, "satz adopt is the only writer");
    assert_eq!(import_id.typed, AttrType::Unknown);
    assert!(row(auditors, "display_name").editable);

    let org = child(&m.outline, "google_organization_iam_member");
    let grant = &org.children[0];
    assert_eq!(grant.kind, ResourceKind::MemberGrant);
    assert_eq!(
        grant.key_parts,
        vec![
            StrPart::Lit("group:gcp-auditors@".to_string()),
            param("customer_domain", "example.com")
        ]
    );
    assert_eq!(
        grant.attrs[0].mode,
        EditMode::Source,
        "a conditional role is an object"
    );
    match &grant.attrs[0].value {
        SourceValue::List(items) => assert!(matches!(items[2], SourceValue::Obj), "{items:?}"),
        other => panic!("{other:?}"),
    }

    let infra = child(
        &child(&child(&m.outline, "google_folder").children, "infra").children,
        "google_project",
    );
    let project = child(&infra.children, "infra");
    let buckets = child(&project.children, "google_storage_bucket");
    let audit = child(&buckets.children, "audit_logs");
    let name = row(audit, "name");
    assert_eq!(
        name.value,
        SourceValue::Str {
            raw: "{customer_shortname}-audit-logs".to_string(),
            parts: vec![
                param("customer_shortname", "corp"),
                StrPart::Lit("-audit-logs".to_string())
            ]
        }
    );
    assert_eq!(name.mode, EditMode::Source);
    let rules = row(audit, "lifecycle_rule");
    assert_eq!(rules.mode, EditMode::Source);
    assert!(
        matches!(&rules.value, SourceValue::List(items) if items.len() == 2 && items.iter().all(|i| *i == SourceValue::Obj))
    );
    assert!(
        audit.missing_required.is_empty(),
        "{:?}",
        audit.missing_required
    );

    let bucket_grants: Vec<&ResourceNode> = project
        .children
        .iter()
        .filter(|n| n.key == "google_storage_bucket_iam_member")
        .collect();
    assert_eq!(
        bucket_grants.len(),
        2,
        "the labelled form and the member map"
    );
    let labelled = child(&bucket_grants[0].children, "auditors_read");
    assert_eq!(labelled.kind, ResourceKind::Resource);
    let bucket = row(labelled, "bucket");
    assert!(bucket.required);
    assert_eq!(
        bucket.value,
        SourceValue::Str {
            raw: "${{google_storage_bucket.audit_logs.name}}".to_string(),
            parts: vec![StrPart::TfRef(
                "google_storage_bucket.audit_logs.name".to_string()
            )]
        }
    );
    assert_eq!(bucket.mode, EditMode::Source);
    assert!(
        labelled.missing_required.is_empty(),
        "{:?}",
        labelled.missing_required
    );

    let map = bucket_grants[1];
    assert_eq!(map.kind, ResourceKind::ResourceMap);
    assert_eq!(row(map, "bucket").mode, EditMode::Source);
    assert_eq!(map.children.len(), 1);
    assert_eq!(map.children[0].kind, ResourceKind::MemberGrant);
    assert_eq!(map.children[0].key, "group:gcp-auditors@{customer_domain}");

    let project_grants = child(&project.children, "google_project_iam_member");
    assert_eq!(project_grants.children[0].kind, ResourceKind::MemberGrant);
}

#[test]
fn missing_required_names_what_satz_does_not_derive() {
    let text = "estate x\n\nparams {\n}\n\ngoogle_storage_bucket {\n  b {\n    location = \"EU\"\n  }\n}\n\ngoogle_folder {\n  f {\n    google_project {\n      p {\n        name = \"n\"\n        labels {\n          team = \"platform\"\n        }\n        lifecycle {\n          ignore_changes = [\"labels\"]\n        }\n      }\n    }\n  }\n}\n\ngoogle_cloud_identity_group {\n  g {\n    display_name = \"G\"\n  }\n}\n\ngoogle_nothing {\n  x {\n    a = 1\n  }\n}\n";
    let m = inline(text, &Env::new());

    let bucket = child(&child(&m.outline, "google_storage_bucket").children, "b");
    assert_eq!(bucket.missing_required, ["name"]);

    let folder = child(&child(&m.outline, "google_folder").children, "f");
    assert!(
        folder.missing_required.is_empty(),
        "{:?}",
        folder.missing_required
    );
    let project = child(&child(&folder.children, "google_project").children, "p");
    assert_eq!(project.missing_required, ["project_id"]);
    let labels = child(&project.children, "labels");
    assert_eq!(
        labels.kind,
        ResourceKind::NestedBlock,
        "a map attribute in block form"
    );
    let team = row(labels, "team");
    assert_eq!(team.typed, AttrType::String, "the map's element type");
    assert!(team.editable);
    let lifecycle = child(&project.children, "lifecycle");
    assert_eq!(
        lifecycle.kind,
        ResourceKind::NestedBlock,
        "Terraform's own block"
    );
    assert!(row(lifecycle, "ignore_changes").editable);

    let group = child(
        &child(&m.outline, "google_cloud_identity_group").children,
        "g",
    );
    assert!(
        group.missing_required.is_empty(),
        "{:?}",
        group.missing_required
    );

    let nothing = child(&m.outline, "google_nothing");
    assert_eq!(nothing.kind, ResourceKind::Unknown);
    let x = child(&nothing.children, "x");
    assert_eq!(x.kind, ResourceKind::Unknown);
    assert!(!row(x, "a").editable, "an unknown block is never edited");
}

#[test]
fn without_a_registry_everything_typed_is_unknown() {
    let estate = fixture();
    let main = estate.yaml_dir().join("smoke.satz");
    let text = std::fs::read_to_string(&main).unwrap();
    let cst = Cst::parse(&text).unwrap();
    let env = estate.params(&main).unwrap();
    let dir = Path::new("/nowhere/schemas");
    let m = EstateModel::build(
        &main,
        &cst,
        Err(dir),
        &env,
        &no_questions(&main),
        &PackDecls::read(&main, &cst, &estate.loader(&main)),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(m.schema, SchemaStatus::Missing(dir.to_path_buf()));

    fn all_unknown(node: &ResourceNode) -> bool {
        node.kind == ResourceKind::Unknown
            && node
                .attrs
                .iter()
                .all(|a| !a.editable && a.typed == AttrType::Unknown)
            && node.children.iter().all(all_unknown)
    }
    for node in &m.outline {
        if node.key == "terraform" || node.key == "providers" {
            assert_eq!(node.kind, ResourceKind::Config, "{}", node.key);
        } else {
            assert!(all_unknown(node), "{} is not all unknown", node.key);
        }
    }
    let folder = child(&m.outline, "google_folder");
    let logging = child(&folder.children, "logging");
    assert_eq!(logging.uses.len(), 2, "the use lines are still found");
}
