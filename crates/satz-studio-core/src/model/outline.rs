//! The outline: every top-level block of the file classified as satz classifies it.
//! `terraform` and `providers` are configuration (`EstateResolver::resolve`,
//! `vendor/satz/src/main.rs`); a key that is exactly a type in the registry is a resource
//! map, or one resource when the block carries a name; inside a resource, a key spelled
//! like a provider type is a child resource (`split_body`'s `is_child`,
//! `vendor/satz/crates/satz-core/src/pipeline.rs`), a key the schema declares is a
//! nested block, and a string key with a list value inside a grant type is a member
//! grant. Everything else is `Unknown`: shown, never edited. Without a registry every
//! decision that needs one is `Unknown`.

use std::collections::BTreeSet;

use satz_core::pipeline::Env;

use super::value::{decode, mode_of};
use super::{AttrRow, ModelError, ResourceKind, ResourceNode, StrPart, decode_string};
use crate::cst::{Cst, NodeId, NodeKind, Span, UseLine, ValueKind};
use crate::schema::{AttrType, BlockSchema, ResourceRegistry, ResourceSchema};

/// The top-level keys satz's resolver calls configuration, never a resource type.
const CONFIG_KEYS: &[&str] = &["terraform", "providers", "variables", "include"];

/// The outline and the `use` lines outside every top-level block.
pub(super) fn build(
    cst: &Cst,
    registry: Option<&ResourceRegistry>,
    env: &Env,
    uses: &[UseLine],
) -> Result<(Vec<ResourceNode>, Vec<UseLine>), ModelError> {
    let w = Walker {
        cst,
        registry,
        env,
        uses,
    };
    let mut outline = Vec::new();
    let mut spans = Vec::new();
    for &c in &cst.node(cst.root()).children {
        if let Some(b) = w.block(c) {
            spans.push(cst.node(c).span);
            outline.push(w.top(b)?);
        }
    }
    let top_uses = uses
        .iter()
        .filter(|u| !spans.iter().any(|s| within(u.span, *s)))
        .cloned()
        .collect();
    Ok((outline, top_uses))
}

fn within(inner: Span, outer: Span) -> bool {
    inner.start >= outer.start && inner.end <= outer.end
}

/// The attributes satz's emitter writes itself for a type, so their absence in the
/// file is not a missing argument (`vendor/satz/src/emitter.rs`, `emit_shared.rs`): a
/// folder's parent is its nesting and its display name falls back to its label; a
/// project's name falls back to its id and its parent and billing account to the
/// context; a group's key, parent and labels come from the customer; an org policy's
/// parent is its nesting; every other type inherits the narrowest of project, folder
/// and organisation into whichever of these attributes its schema has.
fn derived(tf_type: &str) -> &'static [&'static str] {
    match tf_type {
        "google_folder" => &["display_name", "parent"],
        "google_project" => &["billing_account", "folder_id", "name", "org_id"],
        "google_cloud_identity_group" => &[
            "display_name",
            "group_key",
            "initial_group_config",
            "labels",
            "parent",
        ],
        "google_org_policy_policy" => &["parent"],
        _ => &[
            "folder",
            "folder_id",
            "org_id",
            "organization",
            "project",
            "project_id",
        ],
    }
}

/// A grant type: its member map form is `"member" = [roles]`.
fn is_grant(tf_type: &str) -> bool {
    tf_type.ends_with("_iam_member")
}

/// What a block's rows are typed by.
enum Shape<'a> {
    /// a resource, or a nested block the schema declares
    Block(&'a BlockSchema),
    /// an attribute written in block form: the element type types every row
    Elem(AttrType),
    /// no schema: every row is `Unknown`
    None,
}

struct Field {
    typed: AttrType,
    required: bool,
    optional: bool,
    computed: bool,
}

impl Field {
    fn unknown() -> Field {
        Field {
            typed: AttrType::Unknown,
            required: false,
            optional: false,
            computed: false,
        }
    }
}

impl<'a> Shape<'a> {
    fn field(&self, key: &str) -> Field {
        match self {
            Shape::Block(b) => b
                .attributes
                .get(key)
                .map_or_else(Field::unknown, |a| Field {
                    typed: a.attr_type(),
                    required: a.required,
                    optional: a.optional,
                    computed: a.computed,
                }),
            Shape::Elem(t) => Field {
                typed: elem_of(t, key),
                ..Field::unknown()
            },
            Shape::None => Field::unknown(),
        }
    }

    /// The shape of a block below this one, when the schema declares it: a block type
    /// first, then an attribute written in block form.
    fn child(&self, key: &str) -> Option<Shape<'a>> {
        match self {
            Shape::Block(b) => match b.block_types.get(key) {
                Some(bt) => Some(Shape::Block(&bt.block)),
                None => b.attributes.get(key).map(|a| Shape::Elem(a.attr_type())),
            },
            Shape::Elem(t) => match t {
                AttrType::MapOf(inner) => Some(Shape::Elem((**inner).clone())),
                AttrType::Object(fields) => fields
                    .iter()
                    .find(|(n, _)| n == key)
                    .map(|(_, t)| Shape::Elem(t.clone())),
                _ => None,
            },
            Shape::None => None,
        }
    }
}

/// The type of one entry of an attribute written in block form: a map's element, an
/// object's field.
fn elem_of(t: &AttrType, key: &str) -> AttrType {
    match t {
        AttrType::MapOf(inner) => (**inner).clone(),
        AttrType::Object(fields) => fields
            .iter()
            .find(|(n, _)| n == key)
            .map_or(AttrType::Unknown, |(_, t)| t.clone()),
        _ => AttrType::Unknown,
    }
}

#[derive(Clone, Copy)]
struct BlockRef {
    id: NodeId,
    key: Span,
    name: Option<Span>,
    open: Span,
    close: Span,
    line: u32,
}

struct Walker<'a> {
    cst: &'a Cst,
    registry: Option<&'a ResourceRegistry>,
    env: &'a Env,
    uses: &'a [UseLine],
}

impl Walker<'_> {
    fn block(&self, id: NodeId) -> Option<BlockRef> {
        let node = self.cst.node(id);
        match node.kind {
            NodeKind::Block {
                key,
                name,
                open,
                close,
            } => Some(BlockRef {
                id,
                key,
                name,
                open,
                close,
                line: node.line,
            }),
            _ => None,
        }
    }

    /// The key as written with a string's quotes stripped, and decoded.
    fn key(&self, span: Span, line: u32) -> Result<(String, Vec<StrPart>), ModelError> {
        let text = self.cst.slice(span);
        if text.starts_with('"') {
            decode_string(text, line, self.env)
        } else {
            Ok((text.to_string(), vec![StrPart::Lit(text.to_string())]))
        }
    }

    /// `is_child`: a key spelled like a provider type is a child whether or not it
    /// resolves; a key the registry knows is one unless it is `project_service`.
    fn is_child(&self, key: &str) -> bool {
        key.starts_with("google_")
            || (key != "project_service"
                && self
                    .registry
                    .is_some_and(|r| r.find_resource(key).is_some()))
    }

    fn top(&self, b: BlockRef) -> Result<ResourceNode, ModelError> {
        let key = self.cst.slice(b.key).trim_matches('"');
        if CONFIG_KEYS.contains(&key) {
            return self.plain(b, ResourceKind::Config, true);
        }
        let Some(registry) = self.registry else {
            return self.plain(b, ResourceKind::Unknown, false);
        };
        match registry.find_resource(key) {
            Some((_, schema)) if b.name.is_none() => self.map(b, key, schema),
            Some((_, schema)) => self.resource(b, key, schema),
            None => self.plain(b, ResourceKind::Unknown, false),
        }
    }

    /// A `Config` or an `Unknown` block: untyped rows, every block below it the same
    /// kind.
    fn plain(
        &self,
        b: BlockRef,
        kind: ResourceKind,
        editable: bool,
    ) -> Result<ResourceNode, ModelError> {
        let mut attrs = Vec::new();
        let mut children = Vec::new();
        for &c in &self.cst.node(b.id).children {
            if let Some(cb) = self.block(c) {
                children.push(self.plain(cb, kind, editable)?);
            } else if let Some(row) = self.row(c, &Shape::None, editable)? {
                attrs.push(row);
            }
        }
        self.node(b, kind, None, attrs, children, Vec::new())
    }

    /// `TYPE { name { … } … }`: every block below it is a resource of that type.
    fn map(
        &self,
        b: BlockRef,
        tf_type: &str,
        schema: &ResourceSchema,
    ) -> Result<ResourceNode, ModelError> {
        let shape = Shape::Block(&schema.block);
        let mut attrs = Vec::new();
        let mut children = Vec::new();
        for &c in &self.cst.node(b.id).children {
            if let Some(cb) = self.block(c) {
                children.push(if cb.name.is_none() {
                    self.resource(cb, tf_type, schema)?
                } else {
                    self.plain(cb, ResourceKind::Unknown, false)?
                });
            } else if is_grant(tf_type) && self.is_member_grant(c) {
                if let Some(grant) = self.member_grant(c, tf_type)? {
                    children.push(grant);
                }
            } else if let Some(row) = self.row(c, &shape, true)? {
                attrs.push(row);
            }
        }
        self.node(
            b,
            ResourceKind::ResourceMap,
            Some(tf_type),
            attrs,
            children,
            Vec::new(),
        )
    }

    /// One resource: rows typed by its schema, child resources by nesting, nested
    /// blocks by the schema, `lifecycle` as Terraform's own block.
    fn resource(
        &self,
        b: BlockRef,
        tf_type: &str,
        schema: &ResourceSchema,
    ) -> Result<ResourceNode, ModelError> {
        let shape = Shape::Block(&schema.block);
        let mut attrs = Vec::new();
        let mut children = Vec::new();
        for &c in &self.cst.node(b.id).children {
            if let Some(cb) = self.block(c) {
                let key = self.cst.slice(cb.key).trim_matches('"').to_string();
                children.push(if self.is_child(&key) {
                    match self.registry.and_then(|r| r.find_resource(&key)) {
                        Some((_, s)) if cb.name.is_none() => self.map(cb, &key, s)?,
                        Some((_, s)) => self.resource(cb, &key, s)?,
                        None => self.plain(cb, ResourceKind::Unknown, false)?,
                    }
                } else if let Some(inner) = shape.child(&key) {
                    self.nested(cb, inner)?
                } else if key == "lifecycle" {
                    self.nested(cb, Shape::None)?
                } else {
                    self.plain(cb, ResourceKind::Unknown, false)?
                });
            } else if is_grant(tf_type) && self.is_member_grant(c) {
                if let Some(grant) = self.member_grant(c, tf_type)? {
                    children.push(grant);
                }
            } else if let Some(row) = self.row(c, &shape, true)? {
                attrs.push(row);
            }
        }
        let missing = missing_required(&schema.block, &attrs, &children, derived(tf_type));
        self.node(
            b,
            ResourceKind::Resource,
            Some(tf_type),
            attrs,
            children,
            missing,
        )
    }

    /// A block the schema declares below a resource, or one it does not (`lifecycle`).
    fn nested(&self, b: BlockRef, shape: Shape<'_>) -> Result<ResourceNode, ModelError> {
        let mut attrs = Vec::new();
        let mut children = Vec::new();
        for &c in &self.cst.node(b.id).children {
            if let Some(cb) = self.block(c) {
                let key = self.cst.slice(cb.key).trim_matches('"');
                children.push(match shape.child(key) {
                    Some(inner) => self.nested(cb, inner)?,
                    None => self.plain(cb, ResourceKind::Unknown, false)?,
                });
            } else if let Some(row) = self.row(c, &shape, true)? {
                attrs.push(row);
            }
        }
        let missing = match &shape {
            Shape::Block(s) => missing_required(s, &attrs, &children, &[]),
            _ => Vec::new(),
        };
        self.node(b, ResourceKind::NestedBlock, None, attrs, children, missing)
    }

    /// `"member" = [roles]`: a string key with a list value.
    fn is_member_grant(&self, id: NodeId) -> bool {
        match self.cst.node(id).kind {
            NodeKind::Attr { key, value, .. } => {
                self.cst.slice(key).starts_with('"')
                    && matches!(self.cst.node(value).kind, NodeKind::Value(ValueKind::List))
            }
            _ => false,
        }
    }

    /// The grant as a node of its own: the member is the key, the one row is its roles.
    fn member_grant(&self, id: NodeId, tf_type: &str) -> Result<Option<ResourceNode>, ModelError> {
        let node = self.cst.node(id);
        let NodeKind::Attr { key, .. } = node.kind else {
            return Ok(None);
        };
        let Some(row) = self.row(id, &Shape::None, true)? else {
            return Ok(None);
        };
        let (member, key_parts) = self.key(key, node.line)?;
        Ok(Some(ResourceNode {
            id,
            kind: ResourceKind::MemberGrant,
            key: member,
            key_parts,
            label: None,
            label_parts: None,
            tf_type: Some(tf_type.to_string()),
            line: node.line,
            attrs: vec![row],
            children: Vec::new(),
            uses: Vec::new(),
            missing_required: Vec::new(),
        }))
    }

    /// One `key = value` as a row; `None` for anything that is not an attribute, or
    /// whose value the grammar could not read whole.
    fn row(
        &self,
        id: NodeId,
        shape: &Shape<'_>,
        block_editable: bool,
    ) -> Result<Option<AttrRow>, ModelError> {
        let node = self.cst.node(id);
        let NodeKind::Attr { key, value, .. } = node.kind else {
            return Ok(None);
        };
        let Some(v) = decode(self.cst, value, self.env)? else {
            return Ok(None);
        };
        let key = self.cst.slice(key).trim_matches('"').to_string();
        let f = shape.field(&key);
        let editable =
            block_editable && key != "import-id" && !(f.computed && !f.optional && !f.required);
        Ok(Some(AttrRow {
            id,
            key,
            mode: mode_of(&v),
            value: v,
            typed: f.typed,
            required: f.required,
            optional: f.optional,
            computed: f.computed,
            editable,
            line: node.line,
        }))
    }

    /// The `use` lines inside `b` and not inside a block below it.
    fn uses_in(&self, b: BlockRef) -> Vec<UseLine> {
        let body = Span {
            start: b.open.end,
            end: b.close.start,
        };
        let below: Vec<Span> = self
            .cst
            .node(b.id)
            .children
            .iter()
            .filter_map(|&c| self.block(c))
            .map(|cb| self.cst.node(cb.id).span)
            .collect();
        self.uses
            .iter()
            .filter(|u| within(u.span, body) && !below.iter().any(|s| within(u.span, *s)))
            .cloned()
            .collect()
    }

    fn node(
        &self,
        b: BlockRef,
        kind: ResourceKind,
        tf_type: Option<&str>,
        attrs: Vec<AttrRow>,
        children: Vec<ResourceNode>,
        missing_required: Vec<String>,
    ) -> Result<ResourceNode, ModelError> {
        let (key, key_parts) = self.key(b.key, b.line)?;
        let (label, label_parts) = match b.name {
            Some(s) => {
                let (l, p) = self.key(s, b.line)?;
                (Some(l), Some(p))
            }
            None => (None, None),
        };
        Ok(ResourceNode {
            id: b.id,
            kind,
            key,
            key_parts,
            label,
            label_parts,
            tf_type: tf_type.map(str::to_string),
            line: b.line,
            attrs,
            children,
            uses: self.uses_in(b),
            missing_required,
        })
    }
}

/// The required attributes and blocks of `schema` with neither a row nor a child here,
/// minus `derived` (an attribute or a block, `group_key` being one); sorted.
fn missing_required(
    schema: &BlockSchema,
    attrs: &[AttrRow],
    children: &[ResourceNode],
    derived: &[&str],
) -> Vec<String> {
    let present: BTreeSet<&str> = attrs
        .iter()
        .map(|a| a.key.as_str())
        .chain(children.iter().map(|c| c.key.as_str()))
        .collect();
    let mut missing: Vec<String> = schema
        .attributes
        .iter()
        .filter(|(k, a)| {
            a.required && !present.contains(k.as_str()) && !derived.contains(&k.as_str())
        })
        .map(|(k, _)| k.clone())
        .chain(
            schema
                .block_types
                .iter()
                .filter(|(k, b)| {
                    b.min_items.unwrap_or(0) >= 1
                        && !present.contains(k.as_str())
                        && !derived.contains(&k.as_str())
                })
                .map(|(k, _)| k.clone()),
        )
        .collect();
    missing.sort();
    missing
}
