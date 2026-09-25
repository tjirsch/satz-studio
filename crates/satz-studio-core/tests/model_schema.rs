//! `ResourceRegistry::load_all` over satz's schema fixture, `AttrType` decoding, and
//! the refusal of a directory with no schema in it.

use std::path::PathBuf;

use satz_studio_core::schema::{AttrType, ResourceRegistry, SchemaError};
use serde_json::json;

fn schemas() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/satz/tests/schemas")
}

#[test]
fn load_all_reads_every_type_of_the_fixture() {
    let registry = ResourceRegistry::load_all(&schemas()).unwrap();
    assert_eq!(registry.resources.len(), 47);
    assert_eq!(
        registry.providers(),
        vec!["registry.opentofu.org/hashicorp/google".to_string()]
    );

    let (provider, bucket) = registry.find_resource("google_storage_bucket").unwrap();
    assert_eq!(provider, "registry.opentofu.org/hashicorp/google");
    let ubla = &bucket.block.attributes["uniform_bucket_level_access"];
    assert_eq!(ubla.attr_type(), AttrType::Bool);
    assert!(ubla.optional && ubla.computed && !ubla.required);
    assert!(bucket.block.block_types.contains_key("versioning"));

    let (_, project) = registry.find_resource("google_project").unwrap();
    assert!(project.block.attributes["project_id"].required);

    let (_, folder) = registry.find_resource("google_folder").unwrap();
    assert_eq!(
        folder.block.attributes["configured_capabilities"].attr_type(),
        AttrType::ListOf(Box::new(AttrType::String))
    );

    assert!(
        registry.find_resource("storage_bucket").is_none(),
        "exact lookup only: no `google_` prefix is tried"
    );
}

#[test]
fn attr_types_decode_and_print_terraforms_spelling() {
    assert_eq!(
        AttrType::from_json(Some(&json!("string"))),
        AttrType::String
    );
    assert_eq!(
        AttrType::from_json(Some(&json!("number"))),
        AttrType::Number
    );
    assert_eq!(AttrType::from_json(Some(&json!("bool"))), AttrType::Bool);
    assert_eq!(
        AttrType::from_json(Some(&json!(["list", "string"]))),
        AttrType::ListOf(Box::new(AttrType::String))
    );
    assert_eq!(
        AttrType::from_json(Some(&json!(["set", "number"]))),
        AttrType::SetOf(Box::new(AttrType::Number))
    );
    assert_eq!(
        AttrType::from_json(Some(&json!(["map", ["list", "bool"]]))),
        AttrType::MapOf(Box::new(AttrType::ListOf(Box::new(AttrType::Bool))))
    );

    let object = AttrType::from_json(Some(&json!(["object", {"b": "number", "a": "string"}])));
    assert_eq!(
        object,
        AttrType::Object(vec![
            ("a".to_string(), AttrType::String),
            ("b".to_string(), AttrType::Number)
        ]),
        "fields sorted by name"
    );
    assert_eq!(object.to_string(), "object({a=string, b=number})");
    assert_eq!(
        AttrType::ListOf(Box::new(object)).to_string(),
        "list(object({a=string, b=number}))"
    );
    assert_eq!(
        AttrType::MapOf(Box::new(AttrType::String)).to_string(),
        "map(string)"
    );

    assert_eq!(AttrType::from_json(None), AttrType::Unknown);
    assert_eq!(
        AttrType::from_json(Some(&json!("dynamic"))),
        AttrType::Unknown
    );
    assert_eq!(
        AttrType::from_json(Some(&json!(["tuple", ["string"]]))),
        AttrType::Unknown
    );
    assert_eq!(
        AttrType::from_json(Some(&json!(["object", "string"]))),
        AttrType::Unknown
    );
    assert_eq!(AttrType::Unknown.to_string(), "unknown");

    assert!(AttrType::String.is_scalar());
    assert!(AttrType::Bool.is_scalar());
    assert!(!AttrType::ListOf(Box::new(AttrType::String)).is_scalar());
    assert!(!AttrType::Unknown.is_scalar());
}

#[test]
fn a_directory_with_no_schema_is_missing_and_names_the_remedy() {
    let dir = tempfile::tempdir().unwrap();
    let nowhere = dir.path().join("none");
    match ResourceRegistry::load_all(&nowhere) {
        Err(SchemaError::Missing(p)) => assert_eq!(p, nowhere),
        other => panic!("expected Missing, got {other:?}"),
    }

    let err = ResourceRegistry::load_all(dir.path()).unwrap_err();
    assert!(matches!(err, SchemaError::Missing(_)), "{err}");
    assert!(err.to_string().contains("satz update-schema"), "{err}");

    std::fs::write(
        dir.path().join("empty.json"),
        r#"{"provider_schemas": {"x": {"resource_schemas": {}}}}"#,
    )
    .unwrap();
    assert!(
        matches!(
            ResourceRegistry::load_all(dir.path()),
            Err(SchemaError::Missing(_))
        ),
        "a file with no resource type is no schema either"
    );
}

#[test]
fn a_file_that_is_not_a_schema_names_itself() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("x.json"), "{}").unwrap();
    match ResourceRegistry::load_all(dir.path()) {
        Err(SchemaError::Parse { path, .. }) => assert!(path.ends_with("x.json"), "{path:?}"),
        other => panic!("expected Parse, got {other:?}"),
    }
}
