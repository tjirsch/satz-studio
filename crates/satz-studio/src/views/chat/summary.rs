//! The words the chat shows for a tool call in place of its JSON: the one line a tool
//! card carries about the result, and the sentence and the short list of arguments the
//! approval card says the call will run with. The JSON itself is the debug log's.

use serde_json::Value;

/// What the approval card says for a call without arguments, in place of `{}`.
pub const NO_ARGUMENTS: &str = "no arguments";

/// How long one shown value may be before it is cut.
const MAX_VALUE: usize = 60;
/// How many members of an object or items of an array are named before they are counted.
const MAX_NAMED: usize = 3;
/// How many `key: value` parts a result summary carries.
const MAX_PARTS: usize = 4;

/// The one line a tool card shows about a result, from the text the model read. A
/// refusal is its first line of prose. A result that is a JSON object is its top-level
/// counts — `questions: 28`, one part per array — or, with no array in it, its short
/// scalar fields; anything else is its first line.
pub fn summary_line(body: &str, is_error: bool) -> String {
    if is_error {
        return body
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with(['{', '[']))
            .map(cut)
            .unwrap_or_else(|| "the call was refused".to_string());
    }
    match serde_json::from_str::<Value>(body) {
        Ok(Value::Object(map)) => {
            let counts: Vec<String> = map
                .iter()
                .filter_map(|(k, v)| v.as_array().map(|a| format!("{k}: {}", a.len())))
                .take(MAX_PARTS)
                .collect();
            if !counts.is_empty() {
                return counts.join(", ");
            }
            let scalars: Vec<String> = map
                .iter()
                .filter_map(|(k, v)| scalar(v).map(|s| format!("{k}: {s}")))
                .take(MAX_PARTS)
                .collect();
            if !scalars.is_empty() {
                scalars.join(", ")
            } else if map.is_empty() {
                "an empty result".to_string()
            } else {
                format!("{} fields", map.len())
            }
        }
        Ok(Value::Array(items)) => format!("{} items", items.len()),
        Ok(other) => scalar(&other).unwrap_or_else(|| "a result".to_string()),
        Err(_) => body
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(cut)
            .unwrap_or_else(|| "no output".to_string()),
    }
}

/// What the approval card says a call will do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalText {
    /// `Run satz_interview on C0example.satz with:`, or `… with no arguments.`
    pub sentence: String,
    /// one `name: value` line per argument; empty for a call without any
    pub arguments: Vec<String>,
}

/// The approval card's words for `tool` called with `input`. The estate is the one the
/// arguments name under `estate`, else `open_estate`; the `estate` argument is then not
/// repeated in the list.
pub fn approval_text(tool: &str, open_estate: &str, input: &Value) -> ApprovalText {
    let Some(args) = input.as_object() else {
        return ApprovalText {
            sentence: format!(
                "Run {tool} on {open_estate} with arguments that are not a JSON object; the debug log shows them."
            ),
            arguments: Vec::new(),
        };
    };
    let estate = args
        .get("estate")
        .and_then(Value::as_str)
        .unwrap_or(open_estate);
    let arguments: Vec<String> = args
        .iter()
        .filter(|(k, v)| !(k.as_str() == "estate" && v.is_string()))
        .map(|(k, v)| format!("{k}: {}", shown(v)))
        .collect();
    let sentence = if arguments.is_empty() {
        format!("Run {tool} on {estate} with {NO_ARGUMENTS}.")
    } else {
        format!("Run {tool} on {estate} with:")
    };
    ApprovalText {
        sentence,
        arguments,
    }
}

/// One argument's value in words: a scalar as itself, a small array or object of
/// scalars spelled out, a larger one counted.
fn shown(value: &Value) -> String {
    if let Some(s) = scalar(value) {
        return s;
    }
    match value {
        Value::Array(items) if items.is_empty() => "none".to_string(),
        Value::Array(items) => {
            let named: Option<Vec<String>> = items.iter().map(scalar).collect();
            match named {
                Some(named) if named.len() <= MAX_NAMED => named.join(", "),
                _ => format!("{} items", items.len()),
            }
        }
        Value::Object(map) if map.is_empty() => "none".to_string(),
        Value::Object(map) => {
            let named: Option<Vec<String>> = map
                .iter()
                .map(|(k, v)| scalar(v).map(|s| format!("{k} = {s}")))
                .collect();
            match named {
                Some(named) if named.len() <= MAX_NAMED => named.join(", "),
                _ => format!("{} entries", map.len()),
            }
        }
        _ => unreachable!("every scalar is handled above"),
    }
}

/// A scalar in words — a string without its quotes, cut when long — or `None` for an
/// array or an object.
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::Null => Some("none".to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) => Some(cut(s.lines().next().unwrap_or_default())),
        Value::Array(_) | Value::Object(_) => None,
    }
}

fn cut(text: &str) -> String {
    match text.char_indices().nth(MAX_VALUE) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const QUESTIONS: &str =
        include_str!("../../../../satz-studio-core/tests/fixtures/questions-smoke.json");

    #[test]
    fn a_result_object_is_summed_up_by_its_top_level_counts() {
        let questions: Value = serde_json::from_str(QUESTIONS).unwrap();
        let count = questions["questions"].as_array().unwrap().len();
        assert_eq!(
            summary_line(QUESTIONS, false),
            format!("questions: {count}")
        );
        let check = json!({"estate": "C0example.satz", "addresses": ["google_folder.infra"], "written": [], "findings": []});
        let line = summary_line(&check.to_string(), false);
        assert!(line.contains("addresses: 1"), "{line}");
        assert!(line.contains("findings: 0"), "{line}");
        assert!(!line.contains("C0example"), "{line}");
    }

    #[test]
    fn a_result_without_an_array_is_its_short_fields_and_prose_is_its_first_line() {
        assert_eq!(summary_line("{\"written\": true}", false), "written: true");
        assert_eq!(summary_line("{}", false), "an empty result");
        assert_eq!(
            summary_line("{\"a\": {\"b\": 1}}", false),
            "1 fields",
            "a nested object is counted, never shown"
        );
        assert_eq!(summary_line("[1, 2]", false), "2 items");
        assert_eq!(summary_line("\n  compiled\nmore", false), "compiled");
        assert_eq!(summary_line("", false), "no output");
    }

    #[test]
    fn a_refusal_is_its_first_line_of_prose() {
        let refused = "the estate does not compile\n\n{\n  \"findings\": []\n}";
        assert_eq!(summary_line(refused, true), "the estate does not compile");
        assert_eq!(
            summary_line("{\"findings\": []}", true),
            "the call was refused"
        );
        let long = "x".repeat(100);
        assert_eq!(
            summary_line(&long, true).chars().count(),
            MAX_VALUE + 1,
            "a long line is cut, with an ellipsis"
        );
    }

    #[test]
    fn the_approval_card_says_no_arguments_for_an_empty_object() {
        let text = approval_text("satz_transpile", "C0example.satz", &json!({}));
        assert_eq!(
            text.sentence,
            "Run satz_transpile on C0example.satz with no arguments."
        );
        assert!(text.arguments.is_empty());
        assert!(!text.sentence.contains("{}"));
    }

    #[test]
    fn the_approval_card_lists_the_arguments_in_words() {
        let text = approval_text(
            "satz_interview",
            "C0example.satz",
            &json!({
                "answers": {"deployment_mode": "cloud"},
                "estate": "acme.satz",
                "dry_run": false,
                "packs": ["org-baseline", "logging"],
                "many": [1, 2, 3, 4, 5],
                "nested": {"a": {"b": 1}},
            }),
        );
        assert_eq!(text.sentence, "Run satz_interview on acme.satz with:");
        // the order is the one the arguments arrived in, which a JSON map may keep or not
        let mut arguments = text.arguments.clone();
        arguments.sort();
        assert_eq!(
            arguments,
            vec![
                "answers: deployment_mode = cloud",
                "dry_run: false",
                "many: 5 items",
                "nested: 1 entries",
                "packs: org-baseline, logging",
            ]
        );
        for line in &text.arguments {
            assert!(!line.contains('{'), "{line}");
        }
    }
}
