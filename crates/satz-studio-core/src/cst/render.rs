//! Writing a value as Satz source: quoted and escaped so satz's lexer reads back the
//! string given, lists laid out in the style of the list being replaced.

use super::{Cst, NodeId, NodeKind, StyleCtx, TypedValue, ValueKind, line_start};

pub(super) fn render(value: &TypedValue, ctx: &StyleCtx) -> String {
    match value {
        TypedValue::Str(s) => quote(s),
        TypedValue::Num(n) | TypedValue::Ref(n) | TypedValue::Raw(n) => n.clone(),
        TypedValue::Bool(b) => b.to_string(),
        TypedValue::List(items) => list(items, ctx),
    }
}

/// `s` between double quotes. The lexer's escapes are `\\`, `\"` and `\n`; a `{` opens
/// an interpolation unless doubled, and a doubled `}` reads as one, so both braces are
/// doubled.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '{' => out.push_str("{{"),
            '}' => out.push_str("}}"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `[a, b]` on one line, or one item per line two spaces deeper than the line the list
/// sits on, every item but the last followed by a comma and the last one too when the
/// old list ended so, the `]` on its own line at the list's indentation.
fn list(items: &[TypedValue], ctx: &StyleCtx) -> String {
    if items.is_empty() {
        return "[]".to_string();
    }
    if !ctx.list_multiline {
        let inner: Vec<String> = items.iter().map(|i| render(i, ctx)).collect();
        return format!("[{}]", inner.join(", "));
    }
    let item_indent = format!("{}  ", ctx.indent);
    let inner_ctx = StyleCtx {
        indent: item_indent.clone(),
        ..ctx.clone()
    };
    let mut out = String::from("[\n");
    for (i, item) in items.iter().enumerate() {
        out.push_str(&item_indent);
        out.push_str(&render(item, &inner_ctx));
        if i + 1 < items.len() || ctx.trailing_comma {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(&ctx.indent);
    out.push(']');
    out
}

pub(super) fn style_of(cst: &Cst, node: NodeId) -> StyleCtx {
    let text = cst.text();
    let n = cst.node(node);
    let start = line_start(text, n.span.start);
    let indent: String = text[start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let target = match &n.kind {
        NodeKind::ParamEntry { value, .. } | NodeKind::Attr { value, .. } => *value,
        _ => node,
    };
    let t = cst.node(target);
    let (list_multiline, trailing_comma) = if t.kind == NodeKind::Value(ValueKind::List) {
        let last_item = t
            .children
            .iter()
            .rev()
            .find(|&&c| matches!(cst.node(c).kind, NodeKind::Value(_)));
        let trailing = last_item.is_some_and(|&c| {
            text[cst.node(c).span.end..t.span.end]
                .trim_start()
                .starts_with(',')
        });
        (cst.slice(t.span).contains('\n'), trailing)
    } else {
        (false, false)
    };
    StyleCtx {
        indent,
        list_multiline,
        trailing_comma,
    }
}
