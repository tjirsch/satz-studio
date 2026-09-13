//! A value as the file has it, decoded the way satz's lexer reads it
//! (`vendor/satz/crates/satz-core/src/satz.rs`, the `'"'` arm): `{{` is a literal `{`
//! and `}}` a literal `}`, `{name}` an interpolation, `\"`, `\\` and `\n` the escapes
//! of a single-line string and none of a `"""` string. Inside the literal text every
//! `${…}` is a Terraform reference, as `pipeline::references_in` finds them.

use satz_core::pipeline::Env;

use super::{EditMode, ModelError, SourceValue, StrPart};
use crate::cst::{Cst, NodeId, NodeKind, ValueKind};

/// The value node's [`SourceValue`]. `None` when the grammar could not read it whole —
/// an `Error` node somewhere below it — which the parse diagnostic names; a row over
/// a partial value would edit what is not there.
pub(super) fn decode(cst: &Cst, id: NodeId, env: &Env) -> Result<Option<SourceValue>, ModelError> {
    let node = cst.node(id);
    let NodeKind::Value(kind) = node.kind else {
        return Ok(None);
    };
    if has_error(cst, id) {
        return Ok(None);
    }
    let text = cst.slice(node.span);
    Ok(Some(match kind {
        ValueKind::Str => {
            let (raw, parts) = decode_string(text, node.line, env)?;
            SourceValue::Str { raw, parts }
        }
        ValueKind::Num => SourceValue::Num(text.to_string()),
        ValueKind::Bool => SourceValue::Bool(text == "true"),
        ValueKind::Ref => SourceValue::Ref {
            param: text.to_string(),
            resolved: resolved(env, text, node.line)?,
        },
        ValueKind::List => {
            let mut items = Vec::new();
            for &c in &node.children {
                if let Some(v) = decode(cst, c, env)? {
                    items.push(v);
                }
            }
            SourceValue::List(items)
        }
        ValueKind::Obj => SourceValue::Obj,
    }))
}

fn has_error(cst: &Cst, id: NodeId) -> bool {
    let node = cst.node(id);
    matches!(node.kind, NodeKind::Error { .. }) || node.children.iter().any(|&c| has_error(cst, c))
}

/// The string between the quotes as written, and what satz reads: `quoted` is the
/// whole token, `"…"` or `"""…"""`. A `{name}` is a `Param` with the value `env` binds
/// to it; every `${…}` in the literal text is a `TfRef`. An empty string is one empty
/// `Lit`, as the lexer has it.
pub fn decode_string(
    quoted: &str,
    line: u32,
    env: &Env,
) -> Result<(String, Vec<StrPart>), ModelError> {
    let (triple, body) = unquote(quoted).ok_or_else(|| ModelError::Decode {
        line,
        message: format!("a string token without its quotes: {quoted:?}"),
    })?;
    let b: Vec<char> = body.chars().collect();
    let mut parts = Vec::new();
    let mut lit = String::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            '{' if b.get(i + 1) == Some(&'{') => {
                lit.push('{');
                i += 2;
            }
            '}' if b.get(i + 1) == Some(&'}') => {
                lit.push('}');
                i += 2;
            }
            '{' => {
                if !lit.is_empty() {
                    parts.push(StrPart::Lit(std::mem::take(&mut lit)));
                }
                i += 1;
                let mut name = String::new();
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == '_') {
                    name.push(b[i]);
                    i += 1;
                }
                if b.get(i) != Some(&'}') {
                    return Err(ModelError::Decode {
                        line,
                        message: format!("unterminated interpolation '{{{name}'"),
                    });
                }
                if name.is_empty() {
                    return Err(ModelError::Decode {
                        line,
                        message: "empty interpolation {} (use {{}} for a literal brace)"
                            .to_string(),
                    });
                }
                i += 1;
                let value = resolved(env, &name, line)?;
                parts.push(StrPart::Param {
                    name,
                    resolved: value,
                });
            }
            '\\' if !triple => {
                match b.get(i + 1) {
                    Some('n') => lit.push('\n'),
                    Some('"') => lit.push('"'),
                    Some('\\') => lit.push('\\'),
                    other => {
                        return Err(ModelError::Decode {
                            line,
                            message: format!("unknown escape \\{other:?}"),
                        });
                    }
                }
                i += 2;
            }
            ch => {
                lit.push(ch);
                i += 1;
            }
        }
    }
    if !lit.is_empty() || parts.is_empty() {
        parts.push(StrPart::Lit(lit));
    }
    Ok((body.to_string(), split_references(parts)))
}

/// `(triple, body)` for `"body"` or `"""body"""`.
fn unquote(quoted: &str) -> Option<(bool, &str)> {
    if let Some(body) = quoted
        .strip_prefix("\"\"\"")
        .and_then(|r| r.strip_suffix("\"\"\""))
    {
        return Some((true, body));
    }
    quoted
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .map(|body| (false, body))
}

/// Every `${…}` inside a `Lit` becomes a `TfRef` of the text between the braces,
/// trimmed; an unterminated `${` stays literal.
fn split_references(parts: Vec<StrPart>) -> Vec<StrPart> {
    let mut out = Vec::new();
    for part in parts {
        let StrPart::Lit(text) = part else {
            out.push(part);
            continue;
        };
        let mut rest = text.as_str();
        while let Some(i) = rest.find("${") {
            let after = &rest[i + 2..];
            let Some(end) = after.find('}') else { break };
            if i > 0 {
                out.push(StrPart::Lit(rest[..i].to_string()));
            }
            out.push(StrPart::TfRef(after[..end].trim().to_string()));
            rest = &after[end + 1..];
        }
        if !rest.is_empty() || out.is_empty() {
            out.push(StrPart::Lit(rest.to_string()));
        }
    }
    out
}

/// What `env` binds `name` to, as JSON; `None` when it binds nothing.
fn resolved(env: &Env, name: &str, line: u32) -> Result<Option<serde_json::Value>, ModelError> {
    env.get(name)
        .map(|v| {
            serde_json::to_value(v).map_err(|e| ModelError::Decode {
                line,
                message: format!("param `{name}` resolves to a value JSON cannot carry: {e}"),
            })
        })
        .transpose()
}

/// `Source` when editing the value as text would lose something: a reference, an
/// interpolation, an object, or a list holding one of those.
pub(super) fn mode_of(value: &SourceValue) -> EditMode {
    if needs_source(value) {
        EditMode::Source
    } else {
        EditMode::Value
    }
}

fn needs_source(value: &SourceValue) -> bool {
    match value {
        SourceValue::Str { parts, .. } => parts.iter().any(|p| !matches!(p, StrPart::Lit(_))),
        SourceValue::Num(_) | SourceValue::Bool(_) => false,
        SourceValue::Ref { .. } | SourceValue::Obj => true,
        SourceValue::List(items) => items.iter().any(needs_source),
    }
}

/// How satz reads a param as a gate (`pipeline::truthy`): a bool is itself, a string
/// is true unless empty or `"false"`, nothing is false, anything else is true.
pub fn truthy(v: Option<&serde_yaml::Value>) -> bool {
    match v {
        Some(serde_yaml::Value::Bool(b)) => *b,
        Some(serde_yaml::Value::String(s)) => !s.is_empty() && s != "false",
        Some(serde_yaml::Value::Null) | None => false,
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Env {
        let mut e = Env::new();
        e.insert(
            "customer_shortname".to_string(),
            serde_yaml::Value::String("corp".to_string()),
        );
        e
    }

    #[test]
    fn braces_are_doubled_and_a_reference_is_read_out_of_the_literal() {
        let (raw, parts) =
            decode_string("\"${{google_storage_bucket.audit_logs.name}}\"", 1, &env()).unwrap();
        assert_eq!(raw, "${{google_storage_bucket.audit_logs.name}}");
        assert_eq!(
            parts,
            vec![StrPart::TfRef(
                "google_storage_bucket.audit_logs.name".to_string()
            )]
        );
    }

    #[test]
    fn a_param_is_resolved_and_an_unbound_one_is_none() {
        let (_, parts) = decode_string("\"{customer_shortname}-{other}\"", 1, &env()).unwrap();
        assert_eq!(
            parts,
            vec![
                StrPart::Param {
                    name: "customer_shortname".to_string(),
                    resolved: Some(serde_json::json!("corp"))
                },
                StrPart::Lit("-".to_string()),
                StrPart::Param {
                    name: "other".to_string(),
                    resolved: None
                },
            ]
        );
    }

    #[test]
    fn escapes_decode_in_single_line_strings_only() {
        let (_, parts) = decode_string(r#""a\"b\\c\nd""#, 1, &env()).unwrap();
        assert_eq!(parts, vec![StrPart::Lit("a\"b\\c\nd".to_string())]);
        let (_, parts) = decode_string("\"\"\"a\\nb\"\"\"", 1, &env()).unwrap();
        assert_eq!(parts, vec![StrPart::Lit("a\\nb".to_string())]);
        assert!(decode_string(r#""\q""#, 3, &env()).is_err());
    }

    #[test]
    fn the_empty_string_is_one_empty_literal() {
        let (raw, parts) = decode_string("\"\"", 1, &env()).unwrap();
        assert_eq!(raw, "");
        assert_eq!(parts, vec![StrPart::Lit(String::new())]);
    }

    #[test]
    fn an_unterminated_reference_stays_literal() {
        let (_, parts) = decode_string("\"a ${{b\"", 1, &env()).unwrap();
        assert_eq!(parts, vec![StrPart::Lit("a ${b".to_string())]);
    }

    #[test]
    fn truthy_reads_a_gate_as_satz_does() {
        assert!(truthy(Some(&serde_yaml::Value::Bool(true))));
        assert!(!truthy(Some(&serde_yaml::Value::String(
            "false".to_string()
        ))));
        assert!(!truthy(Some(&serde_yaml::Value::String(String::new()))));
        assert!(truthy(Some(&serde_yaml::Value::String("yes".to_string()))));
        assert!(!truthy(None));
    }
}
