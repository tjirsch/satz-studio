//! The splice and its proof. An edit resolves to a value node, or to a param the block
//! does not bind yet; the rendered text replaces exactly that node's span, an absent
//! param is appended before the block's `}` as satz's `bind` appends it. The new text
//! must satisfy satz-core's parser and must differ from the old tree only at those
//! places — a value spliced in as one node, an appended param as one entry.

use super::{Edit, EditError};
use crate::cst::{
    Cst, CstError, NodeId, NodeKind, Span, StyleCtx, ValueKind, render_value, style_of,
};

/// One replacement in the original text's coordinates: a value's span, or the
/// zero-width point before the params block's `}` where the appended entries go.
struct Splice {
    span: Span,
    text: String,
    append: bool,
}

/// The new text for `edits` over `cst`, proven.
pub(super) fn apply(cst: &Cst, edits: &[Edit]) -> Result<String, EditError> {
    let text = cst.text();

    // 1. Every edit becomes a value target with its rendering, or an appended param.
    let mut values: Vec<(NodeId, String)> = Vec::new();
    let mut appended: Vec<(String, String)> = Vec::new();
    for edit in edits {
        match edit {
            Edit::ReplaceValue { node, value } => {
                let target = value_node(cst, *node)?;
                let rendered = render_value(value, &style_of(cst, target));
                push_value(&mut values, target, rendered)?;
            }
            Edit::ReplaceParam { name, value } => match cst.param(name) {
                Some(entry) => {
                    let target = value_node(cst, entry)?;
                    let rendered = render_value(value, &style_of(cst, target));
                    push_value(&mut values, target, rendered)?;
                }
                None => {
                    if cst.params().is_none() {
                        return Err(EditError::NoParamsBlock);
                    }
                    if appended.iter().any(|(n, _)| n == name) {
                        return Err(EditError::Duplicate(format!("param `{name}`")));
                    }
                    appended.push((name.clone(), render_value(value, &StyleCtx::default())));
                }
            },
        }
    }
    for &(inner, _) in &values {
        for &(outer, _) in &values {
            if inner != outer && contains(cst, outer, inner) {
                return Err(EditError::Nested { inner, outer });
            }
        }
    }

    // 2. The splices: one per value, and one at the params block's `}` holding every
    //    appended entry in order — `bind` applied once per name.
    let mut splices: Vec<Splice> = values
        .iter()
        .map(|(id, rendered)| Splice {
            span: cst.node(*id).span,
            text: rendered.clone(),
            append: false,
        })
        .collect();
    if !appended.is_empty() {
        let params = cst.params().ok_or(EditError::NoParamsBlock)?;
        let close = closing_brace(cst, params)?;
        let mut ins = String::new();
        let mut ends_with_newline = text[..close].ends_with('\n');
        for (name, rendered) in &appended {
            if !ends_with_newline {
                ins.push('\n');
            }
            ins.push_str(&format!("  {name} = {rendered}\n"));
            ends_with_newline = true;
        }
        splices.push(Splice {
            span: Span {
                start: close,
                end: close,
            },
            text: ins,
            append: true,
        });
    }

    // 3. Where each splice lands in the new text, walking upwards; then the splices
    //    themselves, from the highest start down, so every original span stays valid.
    splices.sort_by_key(|s| s.span.start);
    let mut delta = 0isize;
    let mut new_spans: Vec<Span> = Vec::with_capacity(splices.len());
    for s in &splices {
        let start = (s.span.start as isize + delta) as usize;
        new_spans.push(Span {
            start,
            end: start + s.text.len(),
        });
        delta += s.text.len() as isize - s.span.len() as isize;
    }
    let mut new = text.to_string();
    for s in splices.iter().rev() {
        new.replace_range(s.span.start..s.span.end, &s.text);
    }

    // 4. The proof: satz-core accepts the text, and the tree differs only at the holes.
    satz_core::satz::parse(&new).map_err(|e| EditError::Syntax {
        line: e.line as u32,
        message: e.msg,
    })?;
    let after = Cst::parse(&new)?;
    let placed = || splices.iter().zip(&new_spans);
    let value_holes: Vec<Span> = placed()
        .filter(|(s, _)| !s.append)
        .map(|(_, n)| *n)
        .collect();
    let appended_range = placed().find(|(s, _)| s.append).map(|(_, n)| *n);
    let names: Vec<String> = appended.into_iter().map(|(n, _)| n).collect();
    let holes: Vec<NodeId> = values.into_iter().map(|(id, _)| id).collect();
    let old = old_signatures(
        cst,
        &holes,
        cst.params().filter(|_| !names.is_empty()),
        &names,
    );
    let fresh = new_signatures(&after, &value_holes, appended_range);
    if let Some(line) = first_difference(&old, &fresh) {
        return Err(EditError::ChangedElsewhere { line });
    }
    if names.is_empty() {
        return Ok(new);
    }

    // 5. An append lays the block out as satz's `bind` does after one, and that layout
    //    moves whitespace only: the tree before and after it is the same node for node.
    let laid_out = align_params(&new);
    satz_core::satz::parse(&laid_out).map_err(|e| EditError::Syntax {
        line: e.line as u32,
        message: e.msg,
    })?;
    let unaligned: Vec<Sig> = new_signatures(&after, &[], None)
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    let aligned = new_signatures(&Cst::parse(&laid_out)?, &[], None);
    if let Some(line) = first_difference(&unaligned, &aligned) {
        return Err(EditError::ChangedElsewhere { line });
    }
    Ok(laid_out)
}

/// The params block as `satz fmt` lays it out, spliced into text whose rest is left
/// alone — what satz's `bind` does after it appends an answer, so the `=` column of the
/// block survives the append, a column a hand edit had already broken included. Text
/// the formatter cannot read comes back as it came, as it does in satz.
fn align_params(text: &str) -> String {
    let Ok(formatted) = satz_core::fmt::format(text) else {
        return text.to_string();
    };
    match (params_inner(text), params_inner(&formatted)) {
        (Some((open, close)), Some((fopen, fclose))) => format!(
            "{}{}{}",
            &text[..open],
            &formatted[fopen..fclose],
            &text[close..]
        ),
        _ => text.to_string(),
    }
}

/// The params block between its `{` and its `}`, as byte offsets.
fn params_inner(text: &str) -> Option<(usize, usize)> {
    let cst = Cst::parse(text).ok()?;
    let params = cst.params()?;
    let close = closing_brace(&cst, params).ok()?;
    let start = cst.node(params).span.start;
    let open = start + text[start..close].find('{')? + 1;
    Some((open, close))
}

/// The value node an edit targets: the node itself when it is a value, the value of
/// an attribute or a param entry, nothing else.
fn value_node(cst: &Cst, node: NodeId) -> Result<NodeId, EditError> {
    if node >= cst.nodes().count() {
        return Err(EditError::NodeNotFound(node));
    }
    let target = match &cst.node(node).kind {
        NodeKind::Value(_) => node,
        NodeKind::Attr { value, .. } | NodeKind::ParamEntry { value, .. } => *value,
        _ => return Err(EditError::NotAValue(node)),
    };
    if !matches!(cst.node(target).kind, NodeKind::Value(_)) {
        return Err(EditError::NotAValue(node));
    }
    Ok(target)
}

fn push_value(
    values: &mut Vec<(NodeId, String)>,
    target: NodeId,
    rendered: String,
) -> Result<(), EditError> {
    if values.iter().any(|(id, _)| *id == target) {
        return Err(EditError::Duplicate(format!("node {target}")));
    }
    values.push((target, rendered));
    Ok(())
}

/// `inner` lies inside `outer`: in a tree, that is span containment.
fn contains(cst: &Cst, outer: NodeId, inner: NodeId) -> bool {
    let o = cst.node(outer).span;
    let i = cst.node(inner).span;
    o.start <= i.start && i.end <= o.end
}

/// The offset of the `}` closing the params block, which is the last byte of its span.
fn closing_brace(cst: &Cst, params: NodeId) -> Result<usize, EditError> {
    let n = cst.node(params);
    let close = n
        .span
        .end
        .checked_sub(1)
        .filter(|&c| cst.text()[c..].starts_with('}'));
    close.ok_or_else(|| {
        CstError::Parse {
            line: n.line,
            message: "the `params` node does not end with `}` — the vendored grammar and this edit disagree".to_string(),
        }
        .into()
    })
}

/// One node of the walk, as compared.
#[derive(Debug, PartialEq, Eq)]
enum Sig {
    Node(String),
    /// an edited value: its subtree is not compared
    Hole,
    /// an appended param entry, by name
    Appended(String),
}

/// The kind and what identifies the node: a key, a name, a path and its gate; a scalar
/// value, a comment, an opaque statement and an error node by their whole text. A
/// list or an object is its children.
fn signature(cst: &Cst, id: NodeId) -> String {
    let n = cst.node(id);
    let s = |sp: Span| cst.slice(sp);
    match &n.kind {
        NodeKind::Document => "document".to_string(),
        NodeKind::Header { keyword, name } => format!("header {keyword} {name}"),
        NodeKind::Params => "params".to_string(),
        NodeKind::ParamEntry { name, .. } => format!("param {}", s(*name)),
        NodeKind::Use { path, as_key, when } => {
            format!(
                "use {} as {:?} when {:?}",
                s(*path),
                as_key.map(s),
                when.map(s)
            )
        }
        NodeKind::Block { key, name, .. } => format!("block {} {:?}", s(*key), name.map(s)),
        NodeKind::Attr { key, .. } => format!("attr {}", s(*key)),
        NodeKind::Value(ValueKind::List) => "list".to_string(),
        NodeKind::Value(ValueKind::Obj) => "obj".to_string(),
        NodeKind::Value(kind) => format!("{kind:?} {}", s(n.span)),
        NodeKind::Opaque { statement } => format!("{statement} {}", s(n.span)),
        NodeKind::Comment => format!("comment {}", s(n.span)),
        NodeKind::Error { message } => format!("error {message} {}", s(n.span)),
    }
}

/// The original tree with the edited values as holes and the appended names after the
/// last child of the params block.
fn old_signatures(
    cst: &Cst,
    holes: &[NodeId],
    params: Option<NodeId>,
    names: &[String],
) -> Vec<Sig> {
    fn walk(
        cst: &Cst,
        id: NodeId,
        holes: &[NodeId],
        params: Option<NodeId>,
        names: &[String],
        out: &mut Vec<Sig>,
    ) {
        if holes.contains(&id) {
            out.push(Sig::Hole);
            return;
        }
        out.push(Sig::Node(signature(cst, id)));
        for &c in &cst.node(id).children {
            walk(cst, c, holes, params, names, out);
        }
        if params == Some(id) {
            out.extend(names.iter().map(|n| Sig::Appended(n.clone())));
        }
    }
    let mut out = Vec::new();
    walk(cst, cst.root(), holes, params, names, &mut out);
    out
}

/// The new tree, each entry with its line: a `Value` node whose span is exactly a
/// spliced span is a hole, a `ParamEntry` inside the appended range is an appended
/// entry of the name it carries.
fn new_signatures(cst: &Cst, value_holes: &[Span], appended: Option<Span>) -> Vec<(Sig, u32)> {
    fn walk(
        cst: &Cst,
        id: NodeId,
        value_holes: &[Span],
        appended: Option<Span>,
        out: &mut Vec<(Sig, u32)>,
    ) {
        let n = cst.node(id);
        match &n.kind {
            NodeKind::Value(_) if value_holes.contains(&n.span) => {
                out.push((Sig::Hole, n.line));
                return;
            }
            NodeKind::ParamEntry { name, .. }
                if appended.is_some_and(|r| r.start <= n.span.start && n.span.end <= r.end) =>
            {
                out.push((Sig::Appended(cst.slice(*name).to_string()), n.line));
                return;
            }
            _ => out.push((Sig::Node(signature(cst, id)), n.line)),
        }
        for &c in &n.children {
            walk(cst, c, value_holes, appended, out);
        }
    }
    let mut out = Vec::new();
    walk(cst, cst.root(), value_holes, appended, &mut out);
    out
}

/// The line, in the new text, of the first node that does not match — the last node's
/// when the new sequence ends early.
fn first_difference(old: &[Sig], new: &[(Sig, u32)]) -> Option<u32> {
    let len = old.len().max(new.len());
    (0..len)
        .find(|&i| old.get(i) != new.get(i).map(|(s, _)| s))
        .map(|i| {
            new.get(i)
                .or_else(|| new.last())
                .map_or(1, |(_, line)| *line)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cst::TypedValue;

    const SRC: &str =
        "estate e\n\nparams {\n  a = \"x\"   # kept\n  b = 1\n}\n\nblock k {\n  attr = [1, 2]\n}\n";

    fn cst() -> Cst {
        Cst::parse(SRC).unwrap()
    }

    #[test]
    fn a_value_is_replaced_in_its_span_only() {
        let cst = cst();
        let a = cst.param("a").unwrap();
        let out = apply(
            &cst,
            &[Edit::ReplaceValue {
                node: a,
                value: TypedValue::Str("yz".into()),
            }],
        )
        .unwrap();
        assert_eq!(
            out,
            SRC.replace("a = \"x\"   # kept", "a = \"yz\"   # kept")
        );
    }

    #[test]
    fn an_absent_param_is_appended_and_the_block_laid_out_as_bind_does() {
        let cst = cst();
        let out = apply(
            &cst,
            &[
                Edit::ReplaceParam {
                    name: "c".into(),
                    value: TypedValue::Bool(true),
                },
                Edit::ReplaceParam {
                    name: "d".into(),
                    value: TypedValue::List(vec![TypedValue::Str("p".into())]),
                },
            ],
        )
        .unwrap();
        // the block as `satz fmt` lays it out — the comment one space after its value —
        // and the rest of the file as it was
        assert_eq!(
            out,
            SRC.replace(
                "  a = \"x\"   # kept\n  b = 1\n}",
                "  a = \"x\" # kept\n  b = 1\n  c = true\n  d = [\"p\"]\n}"
            )
        );
    }

    #[test]
    fn a_block_that_closes_on_the_entry_line_is_opened_by_the_append() {
        let cst = Cst::parse("estate e\nparams { a = 1 }\n").unwrap();
        let out = apply(
            &cst,
            &[Edit::ReplaceParam {
                name: "b".into(),
                value: TypedValue::Num("2".into()),
            }],
        )
        .unwrap();
        assert_eq!(out, "estate e\nparams {\n  a = 1\n  b = 2\n}\n");
    }

    #[test]
    fn an_append_to_a_block_a_hand_edit_misaligned_restores_its_column() {
        let src = "estate e\n\nparams {\n  region = \"eu\"\n  zone     = \"eu-a\"\n}\n";
        let out = apply(
            &Cst::parse(src).unwrap(),
            &[Edit::ReplaceParam {
                name: "project".into(),
                value: TypedValue::Str("p".into()),
            }],
        )
        .unwrap();
        assert_eq!(
            out,
            "estate e\n\nparams {\n  region  = \"eu\"\n  zone    = \"eu-a\"\n  project = \"p\"\n}\n"
        );
    }

    #[test]
    fn the_same_node_twice_and_a_nested_pair_are_refused() {
        let cst = cst();
        let a = cst.param("a").unwrap();
        let err = apply(
            &cst,
            &[
                Edit::ReplaceValue {
                    node: a,
                    value: TypedValue::Num("1".into()),
                },
                Edit::ReplaceParam {
                    name: "a".into(),
                    value: TypedValue::Num("2".into()),
                },
            ],
        )
        .unwrap_err();
        assert!(matches!(err, EditError::Duplicate(_)), "{err}");
        let list = cst
            .nodes()
            .find(|(_, n)| n.kind == NodeKind::Value(ValueKind::List))
            .map(|(id, _)| id)
            .unwrap();
        let item = cst.node(list).children[0];
        let err = apply(
            &cst,
            &[
                Edit::ReplaceValue {
                    node: list,
                    value: TypedValue::List(vec![]),
                },
                Edit::ReplaceValue {
                    node: item,
                    value: TypedValue::Num("3".into()),
                },
            ],
        )
        .unwrap_err();
        assert!(
            matches!(err, EditError::Nested { inner, outer } if inner == item && outer == list),
            "{err}"
        );
    }

    #[test]
    fn a_raw_value_that_adds_a_node_is_changed_elsewhere() {
        let cst = cst();
        let b = cst.param("b").unwrap();
        let err = apply(
            &cst,
            &[Edit::ReplaceValue {
                node: b,
                value: TypedValue::Raw("2 // now with a comment".into()),
            }],
        )
        .unwrap_err();
        assert!(
            matches!(err, EditError::ChangedElsewhere { line: 5 }),
            "{err}"
        );
    }

    #[test]
    fn a_raw_value_satz_cannot_lex_is_a_syntax_error() {
        let cst = cst();
        let b = cst.param("b").unwrap();
        let err = apply(
            &cst,
            &[Edit::ReplaceValue {
                node: b,
                value: TypedValue::Raw("\"open".into()),
            }],
        )
        .unwrap_err();
        assert!(matches!(err, EditError::Syntax { .. }), "{err}");
    }

    #[test]
    fn without_a_params_block_an_append_is_refused() {
        let cst = Cst::parse("estate e\n").unwrap();
        let err = apply(
            &cst,
            &[Edit::ReplaceParam {
                name: "a".into(),
                value: TypedValue::Num("1".into()),
            }],
        )
        .unwrap_err();
        assert!(matches!(err, EditError::NoParamsBlock), "{err}");
    }
}
