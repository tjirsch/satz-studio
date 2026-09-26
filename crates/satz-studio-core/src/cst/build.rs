//! The walk from the tree-sitter tree to the node vector of a [`Cst`](super::Cst):
//! pre-order, so a node's id is smaller than its children's, ids follow document order,
//! and [`nodes_at_line`](super::Cst::nodes_at_line) lists the outermost node of a line
//! first. Keywords and punctuation are not nodes; the ones a node needs (`=`, `{`, `}`)
//! are recorded as spans on it. A comment, an `ERROR` and a `MISSING` token become
//! nodes wherever they stand, so nothing the file carries is invisible to the tree.

use tree_sitter::Node as Ts;

use super::{CstError, Node, NodeId, NodeKind, Span, ValueKind};

/// The node vector for `text`, whose tree-sitter root is `root`. Refuses a node kind
/// the grammar produces and this walk does not map: that is a disagreement between the
/// vendored grammar and this file, never a property of the text.
pub(super) fn build(text: &str, root: Ts<'_>) -> Result<Vec<Node>, CstError> {
    let mut b = Builder {
        text,
        nodes: Vec::new(),
    };
    let id = b.push(
        NodeKind::Document,
        Span {
            start: 0,
            end: text.len(),
        },
        1,
    );
    let mut children = Vec::new();
    for c in kids(root) {
        let child = match b.trivia(c) {
            Some(t) => t,
            None => b.item(c)?,
        };
        children.push(child);
    }
    b.nodes[id].children = children;
    Ok(b.nodes)
}

struct Builder<'t> {
    text: &'t str,
    nodes: Vec<Node>,
}

fn kids(n: Ts<'_>) -> Vec<Ts<'_>> {
    let mut cursor = n.walk();
    n.children(&mut cursor).collect()
}

fn span(n: Ts<'_>) -> Span {
    let r = n.byte_range();
    Span {
        start: r.start,
        end: r.end,
    }
}

fn line(n: Ts<'_>) -> u32 {
    n.start_position().row as u32 + 1
}

/// The first line of `s`, trimmed, cut to a length an error message can carry.
fn excerpt(s: &str) -> String {
    let first = s.lines().next().unwrap_or("").trim();
    let mut out: String = first.chars().take(40).collect();
    if out.len() < first.len() {
        out.push('…');
    }
    out
}

impl<'t> Builder<'t> {
    fn text_of(&self, n: Ts<'_>) -> &'t str {
        &self.text[n.byte_range()]
    }

    fn push(&mut self, kind: NodeKind, span: Span, line: u32) -> NodeId {
        self.nodes.push(Node {
            kind,
            span,
            line,
            children: Vec::new(),
        });
        self.nodes.len() - 1
    }

    fn push_at(&mut self, kind: NodeKind, n: Ts<'_>) -> NodeId {
        self.push(kind, span(n), line(n))
    }

    fn set_children(&mut self, id: NodeId, children: Vec<NodeId>) {
        self.nodes[id].children = children;
    }

    /// A comment, an `ERROR` or a `MISSING` token becomes a node; anything else is
    /// `None` and the caller decides.
    fn trivia(&mut self, n: Ts<'_>) -> Option<NodeId> {
        if n.is_missing() {
            let message = format!("missing `{}`", n.kind());
            return Some(self.push_at(NodeKind::Error { message }, n));
        }
        if n.is_error() {
            let message = format!("cannot read `{}`", excerpt(self.text_of(n)));
            return Some(self.error(n, message));
        }
        if n.kind() == "comment" {
            return Some(self.push_at(NodeKind::Comment, n));
        }
        None
    }

    /// An `Error` node over `n`, with every comment and error inside it as a child.
    fn error(&mut self, n: Ts<'_>, message: String) -> NodeId {
        let id = self.push_at(NodeKind::Error { message }, n);
        let mut out = Vec::new();
        self.collect_trivia(n, &mut out);
        self.set_children(id, out);
        id
    }

    /// Every comment, `ERROR` and `MISSING` below `n`, flattened, in document order.
    fn collect_trivia(&mut self, n: Ts<'_>, out: &mut Vec<NodeId>) {
        for c in kids(n) {
            match self.trivia(c) {
                Some(id) => out.push(id),
                None => self.collect_trivia(c, out),
            }
        }
    }

    /// A named node that is a statement, an entry or a value.
    fn item(&mut self, n: Ts<'_>) -> Result<NodeId, CstError> {
        match n.kind() {
            "header" => Ok(self.header(n)),
            "params" => self.params(n),
            "param" => self.entry(n, "name"),
            "attribute" => self.entry(n, "key"),
            "use_statement" => Ok(self.use_statement(n)),
            "block" => self.block(n),
            "string" | "number" | "boolean" | "reference" | "list" | "object" => self.value(n),
            "claim" | "question" | "action" | "notice" | "offers" | "export" | "interface"
            | "suppress" | "hcl_block" => Ok(self.opaque(n)),
            other => Err(CstError::Parse {
                line: line(n),
                message: format!(
                    "the grammar produced a `{other}` node here, which cst::build does not map — \
                     the vendored grammar and this walk disagree"
                ),
            }),
        }
    }

    /// `estate NAME` / `pack NAME [version "…"] [content]` / `interface "NAME"` (the
    /// header of a generated interface file): the keyword and the name as text — an
    /// interface's name without its quotes, as satz reads it — and a `version` string as
    /// a `Value` child. The quoted name of an interface is no value: satz writes it.
    fn header(&mut self, n: Ts<'_>) -> NodeId {
        let keyword = n
            .child_by_field_name("kind")
            .map(|c| self.text_of(c).to_string())
            .unwrap_or_default();
        let name = n
            .child_by_field_name("name")
            .map(|c| {
                let t = self.text_of(c);
                if c.kind() == "string" {
                    t.strip_prefix('"')
                        .and_then(|t| t.strip_suffix('"'))
                        .unwrap_or(t)
                        .to_string()
                } else {
                    t.to_string()
                }
            })
            .unwrap_or_default();
        let version = n.child_by_field_name("version").map(|c| c.id());
        let id = self.push_at(NodeKind::Header { keyword, name }, n);
        let mut out = Vec::new();
        for c in kids(n) {
            if let Some(t) = self.trivia(c) {
                out.push(t);
            } else if Some(c.id()) == version {
                out.push(self.push_at(NodeKind::Value(ValueKind::Str), c));
            }
        }
        self.set_children(id, out);
        id
    }

    fn params(&mut self, n: Ts<'_>) -> Result<NodeId, CstError> {
        let id = self.push_at(NodeKind::Params, n);
        let mut out = Vec::new();
        for c in kids(n) {
            if let Some(t) = self.trivia(c) {
                out.push(t);
            } else if c.is_named() {
                out.push(self.item(c)?);
            }
        }
        self.set_children(id, out);
        Ok(id)
    }

    /// `name = value` (a param, `key_field` = `name`) or `key = value` (an attribute,
    /// `key_field` = `key`): the key and `=` as spans, the value as a child. A token the
    /// grammar had to insert is an `Error` child at its zero-width span.
    fn entry(&mut self, n: Ts<'_>, key_field: &str) -> Result<NodeId, CstError> {
        let is_param = key_field == "name";
        let what = if is_param { "param" } else { "attribute" };
        let Some(key) = n.child_by_field_name(key_field) else {
            return Ok(self.error(n, format!("a {what} without a name")));
        };
        let Some(value) = n.child_by_field_name("value") else {
            return Ok(self.error(n, format!("a {what} without a value")));
        };
        let children = kids(n);
        let Some(eq) = children.iter().find(|c| c.kind() == "=") else {
            return Ok(self.error(n, format!("a {what} without `=`")));
        };
        let kind = if is_param {
            NodeKind::ParamEntry {
                name: span(key),
                eq: span(*eq),
                value: 0,
            }
        } else {
            NodeKind::Attr {
                key: span(key),
                eq: span(*eq),
                value: 0,
            }
        };
        let id = self.push_at(kind, n);
        let mut out = Vec::new();
        let mut value_id = None;
        for c in children {
            if c.id() == value.id() && !c.is_missing() {
                let v = self.value(c)?;
                value_id = Some(v);
                out.push(v);
            } else if let Some(t) = self.trivia(c) {
                if c.id() == value.id() {
                    value_id = Some(t);
                }
                out.push(t);
            }
        }
        let Some(v) = value_id else {
            return Err(CstError::Parse {
                line: line(n),
                message: format!(
                    "the grammar gave this {what} a value field that is not among its children"
                ),
            });
        };
        match &mut self.nodes[id].kind {
            NodeKind::ParamEntry { value, .. } | NodeKind::Attr { value, .. } => *value = v,
            _ => {}
        }
        self.set_children(id, out);
        Ok(id)
    }

    /// `use "path" [as key] [when gate]`: the string's span with its quotes, the two
    /// identifiers' spans; the first of each when the grammar accepted a repeat.
    fn use_statement(&mut self, n: Ts<'_>) -> NodeId {
        let Some(path) = n.child_by_field_name("path") else {
            return self.error(n, "a use without a path".to_string());
        };
        let as_key = n.child_by_field_name("type").map(span);
        let when = n.child_by_field_name("condition").map(span);
        let id = self.push_at(
            NodeKind::Use {
                path: span(path),
                as_key,
                when,
            },
            n,
        );
        let mut out = Vec::new();
        for c in kids(n) {
            if let Some(t) = self.trivia(c) {
                out.push(t);
            }
        }
        self.set_children(id, out);
        id
    }

    /// `KEY [NAME] { … }`: the key and the name as spans, the body's braces as spans,
    /// the body's entries and comments as children.
    fn block(&mut self, n: Ts<'_>) -> Result<NodeId, CstError> {
        let Some(key) = n.child_by_field_name("key") else {
            return Ok(self.error(n, "a block without a key".to_string()));
        };
        let name = n.child_by_field_name("name").map(span);
        let Some(body) = n.child_by_field_name("body") else {
            return Ok(self.error(n, "a block without a body".to_string()));
        };
        let body_kids = kids(body);
        let Some(open) = body_kids.iter().find(|c| c.kind() == "{") else {
            return Ok(self.error(n, "a block body without `{`".to_string()));
        };
        let Some(close) = body_kids.iter().rev().find(|c| c.kind() == "}") else {
            return Ok(self.error(n, "a block body without `}`".to_string()));
        };
        let kind = NodeKind::Block {
            key: span(key),
            name,
            open: span(*open),
            close: span(*close),
        };
        let id = self.push_at(kind, n);
        let mut out = Vec::new();
        for c in kids(n) {
            if c.id() == body.id() {
                for b in &body_kids {
                    if let Some(t) = self.trivia(*b) {
                        out.push(t);
                    } else if b.is_named() {
                        out.push(self.item(*b)?);
                    }
                }
            } else if let Some(t) = self.trivia(c) {
                out.push(t);
            }
        }
        self.set_children(id, out);
        Ok(id)
    }

    /// A value: scalars are leaves, a list's items and an object's entries are children.
    fn value(&mut self, n: Ts<'_>) -> Result<NodeId, CstError> {
        let kind = match n.kind() {
            "string" => ValueKind::Str,
            "number" => ValueKind::Num,
            "boolean" => ValueKind::Bool,
            "reference" => ValueKind::Ref,
            "list" => ValueKind::List,
            "object" => ValueKind::Obj,
            other => {
                return Err(CstError::Parse {
                    line: line(n),
                    message: format!("the grammar produced a `{other}` node where a value stands"),
                });
            }
        };
        let id = self.push_at(NodeKind::Value(kind), n);
        let mut out = Vec::new();
        match kind {
            ValueKind::List | ValueKind::Obj => {
                for c in kids(n) {
                    if let Some(t) = self.trivia(c) {
                        out.push(t);
                    } else if c.is_named() {
                        out.push(self.item(c)?);
                    }
                }
            }
            _ => self.collect_trivia(n, &mut out),
        }
        self.set_children(id, out);
        Ok(id)
    }

    /// A statement kept whole: its keyword as `statement`, the comments and errors
    /// inside it as children.
    fn opaque(&mut self, n: Ts<'_>) -> NodeId {
        let statement = match n.kind() {
            "hcl_block" => "hcl",
            k => k,
        }
        .to_string();
        let id = self.push_at(NodeKind::Opaque { statement }, n);
        let mut out = Vec::new();
        self.collect_trivia(n, &mut out);
        self.set_children(id, out);
        id
    }
}
