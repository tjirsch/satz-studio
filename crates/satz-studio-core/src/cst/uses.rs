//! The `use` lines of a file: the active ones from `Use` nodes, the commented ones from
//! the `Comment` nodes with the exact shape satz's `template::pack_line` writes and its
//! `interview::uncomment_pack` matches, each with the phase comment directly above it.

use std::collections::HashMap;

use super::{Cst, NodeId, NodeKind, Span, UseLine, UseState, line_start};

pub(super) fn scan(cst: &Cst) -> Vec<UseLine> {
    let text = cst.text();
    // the line comments that stand alone on their line, by line: what a phase comment
    // is made of
    let alone: HashMap<u32, NodeId> = cst
        .nodes()
        .filter(|(_, n)| n.kind == NodeKind::Comment && alone_on_line(text, n.span))
        .map(|(id, n)| (n.line, id))
        .collect();
    let mut out = Vec::new();
    for (_, node) in cst.nodes() {
        let (path, as_key, named, gate, state) = match &node.kind {
            NodeKind::Use {
                path,
                as_key,
                named,
                when,
            } => (
                unquote(cst.slice(*path)).to_string(),
                as_key.map(|s| cst.slice(s).to_string()),
                named.map(|s| cst.slice(s).to_string()),
                when.map(|s| cst.slice(s).to_string()),
                UseState::Active,
            ),
            NodeKind::Comment => match pack_line(cst.slice(node.span)) {
                Some(l) => (
                    l.path.to_string(),
                    l.as_key.map(str::to_string),
                    l.named.map(str::to_string),
                    l.gate.map(str::to_string),
                    UseState::Commented,
                ),
                None => continue,
            },
            _ => continue,
        };
        out.push(UseLine {
            path,
            gate,
            as_key,
            named,
            state,
            span: line_span(text, node.span.start),
            line: node.line,
            phase_comment: phase_comment(cst, &alone, node.line),
        });
    }
    out
}

/// A commented pack line, read: the path between the quotes and the words after it.
pub(super) struct PackLine<'a> {
    pub path: &'a str,
    pub as_key: Option<&'a str>,
    pub named: Option<&'a str>,
    pub gate: Option<&'a str>,
}

/// `// use "<path>"[ as <key>][ named <name>][ when <gate>]` and nothing else — trailing
/// whitespace tolerated. The `as` form is what `uncomment_pack` matches too, by its
/// suffix; `pack_line` never writes it. `named` is a named use's (satz's ADR 0109), in
/// the order satz's own scanner of pack lines reads it.
pub(super) fn pack_line(t: &str) -> Option<PackLine<'_>> {
    let rest = t.strip_prefix("// use \"")?;
    let (path, rest) = rest.split_once('"')?;
    if path.is_empty() {
        return None;
    }
    let mut rest = rest.trim_end();
    let mut as_key = None;
    if let Some(r) = rest.strip_prefix(" as ") {
        let (k, r) = ident_prefix(r)?;
        as_key = Some(k);
        rest = r;
    }
    let mut named = None;
    if let Some(r) = rest.strip_prefix(" named ") {
        let (n, r) = ident_prefix(r)?;
        named = Some(n);
        rest = r;
    }
    let mut gate = None;
    if let Some(r) = rest.strip_prefix(" when ") {
        let (g, r) = ident_prefix(r)?;
        gate = Some(g);
        rest = r;
    }
    if !rest.is_empty() {
        return None;
    }
    Some(PackLine {
        path,
        as_key,
        named,
        gate,
    })
}

/// The identifier at the start of `s` (`[A-Za-z_][A-Za-z0-9_.]*`) and what follows it.
fn ident_prefix(s: &str) -> Option<(&str, &str)> {
    if !s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        return None;
    }
    let n = s
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'.')
        .count();
    Some(s.split_at(n))
}

/// The text between the quotes of a string as written, `"…"` or `"""…"""`.
fn unquote(s: &str) -> &str {
    s.strip_prefix("\"\"\"")
        .and_then(|r| r.strip_suffix("\"\"\""))
        .or_else(|| s.strip_prefix('"').and_then(|r| r.strip_suffix('"')))
        .unwrap_or(s)
}

/// The line `at` is on, without its newline.
fn line_span(text: &str, at: usize) -> Span {
    let start = line_start(text, at);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    Span { start, end }
}

/// A `//` or `#` comment with nothing but whitespace before it on its line.
fn alone_on_line(text: &str, span: Span) -> bool {
    let start = line_start(text, span.start);
    let rest = &text[span.start..];
    text[start..span.start].trim().is_empty() && (rest.starts_with("//") || rest.starts_with('#'))
}

/// The run of comment lines directly above `line` — no blank line between, none of
/// them a pack line, each alone on its line — joined with `\n`, without their comment
/// markers. `None` when the line above is anything else.
fn phase_comment(cst: &Cst, alone: &HashMap<u32, NodeId>, line: u32) -> Option<String> {
    let mut lines: Vec<&str> = Vec::new();
    let mut l = line;
    while l > 1 {
        l -= 1;
        let Some(&id) = alone.get(&l) else { break };
        let t = cst.slice(cst.node(id).span);
        if pack_line(t).is_some() {
            break;
        }
        let Some(body) = caption_body(t) else { break };
        lines.push(body);
    }
    if lines.is_empty() {
        return None;
    }
    lines.reverse();
    Some(lines.join("\n"))
}

/// A line comment without its marker and trailing whitespace; `None` for a block
/// comment, which does not carry a caption.
fn caption_body(t: &str) -> Option<&str> {
    t.strip_prefix("// ")
        .or_else(|| t.strip_prefix("//"))
        .or_else(|| t.strip_prefix("# "))
        .or_else(|| t.strip_prefix('#'))
        .map(str::trim_end)
}
