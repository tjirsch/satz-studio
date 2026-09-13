//! The provider schema, as `satz update-schema` writes it into the estate's
//! `schema_dir`: raw `tofu providers schema -json`. The types and the loader are lifted
//! from satz `src/schema.rs` (MIT, `vendor/satz` at the pinned tag) — satz's binary has
//! no library target, so the app carries its own copy, byte-for-byte in the types.
//! Studio decodes the attribute type here as well ([`AttrType`]); satz leaves it raw.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Schema {
    pub provider_schemas: HashMap<String, ProviderSchema>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProviderSchema {
    pub resource_schemas: HashMap<String, ResourceSchema>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ResourceSchema {
    pub block: BlockSchema,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct BlockSchema {
    #[serde(default)]
    pub attributes: HashMap<String, AttributeSchema>,
    #[serde(default)]
    pub block_types: HashMap<String, BlockTypeSchema>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct BlockTypeSchema {
    pub min_items: Option<u64>,
    pub block: BlockSchema,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AttributeSchema {
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub computed: bool,
    pub default: Option<serde_json::Value>,
    /// Terraform's type expression: `"string"`, `["list", "string"]`, `["object", {…}]`
    #[serde(rename = "type")]
    pub type_: Option<serde_json::Value>,
}

/// A Terraform attribute type, decoded for a typed field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrType {
    String,
    Number,
    Bool,
    ListOf(Box<AttrType>),
    SetOf(Box<AttrType>),
    MapOf(Box<AttrType>),
    Object(Vec<(String, AttrType)>),
    Unknown,
}

impl AttrType {
    pub fn from_json(v: Option<&serde_json::Value>) -> AttrType {
        let _ = v;
        AttrType::Unknown
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("no provider schema in {0} — run `satz update-schema`")]
    Missing(PathBuf),
    #[error("{path}: {source}")]
    Io { path: PathBuf, #[source] source: std::io::Error },
    #[error("{path}: not a provider schema: {source}")]
    Parse { path: PathBuf, #[source] source: serde_json::Error },
    #[error(transparent)]
    Unimplemented(#[from] crate::Unimplemented),
}

/// Every resource type of every `*.json` in the schema directory, keyed by the full
/// Terraform type, with the provider it came from.
#[derive(Debug, Clone, Default)]
pub struct ResourceRegistry {
    pub resources: HashMap<String, (String, ResourceSchema)>,
}

impl ResourceRegistry {
    pub fn load_all(dir: &Path) -> Result<ResourceRegistry, SchemaError> {
        let _ = dir;
        Err(crate::Unimplemented::new("ResourceRegistry::load_all", "U5").into())
    }

    /// Exact lookup only: Satz names Terraform types in full.
    pub fn find_resource(&self, key: &str) -> Option<(&str, &ResourceSchema)> {
        self.resources.get(key).map(|(p, s)| (p.as_str(), s))
    }
}
