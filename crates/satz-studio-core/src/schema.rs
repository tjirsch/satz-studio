//! The provider schema, as `satz update-schema` writes it into the estate's
//! `schema_dir`: raw `tofu providers schema -json`. The types and the loader are lifted
//! from satz `src/schema.rs` (MIT, `vendor/satz` at the pinned tag) — satz's binary has
//! no library target, so the app carries its own copy, byte-for-byte in the types.
//! Studio decodes the attribute type here as well ([`AttrType`]); satz leaves it raw.
//!
//! One departure from satz: a directory with no resource type in it is
//! [`SchemaError::Missing`], never an empty registry. satz tolerates the absence because
//! the schema may be fetched later; the app shows the remedy instead.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
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

impl AttributeSchema {
    /// The attribute's type, decoded.
    pub fn attr_type(&self) -> AttrType {
        AttrType::from_json(self.type_.as_ref())
    }
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
    /// the fields, sorted by name
    Object(Vec<(String, AttrType)>),
    /// a type expression this decoder does not read (`dynamic`, a tuple) — or none
    Unknown,
}

impl AttrType {
    /// `"string"`, `"number"` and `"bool"` are the scalars; `["list", T]`, `["set", T]`
    /// and `["map", T]` the containers; `["object", {name: T, …}]` an object with its
    /// fields sorted by name. Anything else, `None` included, is [`AttrType::Unknown`].
    pub fn from_json(v: Option<&serde_json::Value>) -> AttrType {
        use serde_json::Value;
        match v {
            Some(Value::String(s)) => match s.as_str() {
                "string" => AttrType::String,
                "number" => AttrType::Number,
                "bool" => AttrType::Bool,
                _ => AttrType::Unknown,
            },
            Some(Value::Array(items)) => match items.as_slice() {
                [Value::String(kind), inner] => match kind.as_str() {
                    "list" => AttrType::ListOf(Box::new(AttrType::from_json(Some(inner)))),
                    "set" => AttrType::SetOf(Box::new(AttrType::from_json(Some(inner)))),
                    "map" => AttrType::MapOf(Box::new(AttrType::from_json(Some(inner)))),
                    "object" => match inner {
                        Value::Object(fields) => {
                            let mut out: Vec<(String, AttrType)> = fields
                                .iter()
                                .map(|(k, t)| (k.clone(), AttrType::from_json(Some(t))))
                                .collect();
                            out.sort_by(|a, b| a.0.cmp(&b.0));
                            AttrType::Object(out)
                        }
                        _ => AttrType::Unknown,
                    },
                    _ => AttrType::Unknown,
                },
                _ => AttrType::Unknown,
            },
            _ => AttrType::Unknown,
        }
    }

    /// A string, a number or a bool: what one field holds.
    pub fn is_scalar(&self) -> bool {
        matches!(self, AttrType::String | AttrType::Number | AttrType::Bool)
    }
}

/// Terraform's spelling: `string`, `list(string)`, `map(number)`,
/// `object({a=string, b=bool})`; `unknown` for what is not decoded.
impl fmt::Display for AttrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttrType::String => f.write_str("string"),
            AttrType::Number => f.write_str("number"),
            AttrType::Bool => f.write_str("bool"),
            AttrType::ListOf(t) => write!(f, "list({t})"),
            AttrType::SetOf(t) => write!(f, "set({t})"),
            AttrType::MapOf(t) => write!(f, "map({t})"),
            AttrType::Object(fields) => {
                f.write_str("object({")?;
                for (i, (name, t)) in fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{name}={t}")?;
                }
                f.write_str("})")
            }
            AttrType::Unknown => f.write_str("unknown"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("no provider schema in {0} — run `satz update-schema`")]
    Missing(PathBuf),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: not a provider schema: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// Every resource type of every `*.json` in the schema directory, keyed by the full
/// Terraform type, with the provider it came from.
#[derive(Debug, Clone, Default)]
pub struct ResourceRegistry {
    pub resources: HashMap<String, (String, ResourceSchema)>,
}

impl ResourceRegistry {
    /// Every `*.json` in `dir`, read in name order, each file's `provider_schemas[*]
    /// .resource_schemas` flattened into one map; a type two files both carry is the
    /// later file's. A directory that does not exist, or holds no resource type, is
    /// [`SchemaError::Missing`]; any other failure names its file.
    pub fn load_all(dir: &Path) -> Result<ResourceRegistry, SchemaError> {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(SchemaError::Missing(dir.to_path_buf()));
            }
            Err(e) => {
                return Err(SchemaError::Io {
                    path: dir.to_path_buf(),
                    source: e,
                });
            }
        };
        let mut files = Vec::new();
        for entry in entries {
            let path = entry
                .map_err(|e| SchemaError::Io {
                    path: dir.to_path_buf(),
                    source: e,
                })?
                .path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                files.push(path);
            }
        }
        files.sort();
        let mut resources = HashMap::new();
        for path in files {
            let text = std::fs::read_to_string(&path).map_err(|e| SchemaError::Io {
                path: path.clone(),
                source: e,
            })?;
            let schema: Schema =
                serde_json::from_str(&text).map_err(|e| SchemaError::Parse { path, source: e })?;
            for (provider, ps) in schema.provider_schemas {
                for (tf_type, rs) in ps.resource_schemas {
                    resources.insert(tf_type, (provider.clone(), rs));
                }
            }
        }
        if resources.is_empty() {
            return Err(SchemaError::Missing(dir.to_path_buf()));
        }
        Ok(ResourceRegistry { resources })
    }

    /// Exact lookup only: Satz names Terraform types in full.
    pub fn find_resource(&self, key: &str) -> Option<(&str, &ResourceSchema)> {
        self.resources.get(key).map(|(p, s)| (p.as_str(), s))
    }

    /// The providers the types came from, each once, sorted.
    pub fn providers(&self) -> Vec<String> {
        let set: BTreeSet<&str> = self.resources.values().map(|(p, _)| p.as_str()).collect();
        set.into_iter().map(str::to_string).collect()
    }
}
