//! The tool bridge: MCP tools as Claude tool definitions, and results back.

use super::super::claude::types::{CacheControl, ContentBlock, ToolDef};
use crate::satz::{ToolInfo, ToolOutcome};

/// Every MCP tool as a [`ToolDef`]: the name and `input_schema` verbatim, the
/// description followed, when the tool has an output schema with properties, by what a
/// result and a refusal read as. Sorted by name for a stable cache prefix, the cache
/// breakpoint on the last.
pub fn tool_defs(tools: &[ToolInfo]) -> Vec<ToolDef> {
    let mut defs: Vec<ToolDef> = tools
        .iter()
        .map(|t| ToolDef {
            name: t.name.clone(),
            description: describe(t),
            input_schema: t.input_schema.clone(),
            cache_control: None,
        })
        .collect();
    defs.sort_by(|a, b| a.name.cmp(&b.name));
    if let Some(last) = defs.last_mut() {
        last.cache_control = Some(CacheControl::ephemeral());
    }
    defs
}

/// The description, then `Returns JSON with the keys {a, b, c}. A refusal is prose
/// instead, marked as an error.` — the keys the top level of the output schema names.
/// A refusal is satz's own sentence with `isError`, whatever the output schema says, so
/// the promise of JSON carries its exception with it.
fn describe(tool: &ToolInfo) -> String {
    let mut description = tool.description.trim().to_string();
    let keys: Vec<&str> = tool
        .output_schema
        .as_ref()
        .and_then(|s| s.get("properties"))
        .and_then(serde_json::Value::as_object)
        .map(|p| p.keys().map(String::as_str).collect())
        .unwrap_or_default();
    if keys.is_empty() {
        return description;
    }
    if !description.is_empty() && !description.ends_with('.') {
        description.push('.');
    }
    if !description.is_empty() {
        description.push(' ');
    }
    description.push_str(&format!(
        "Returns JSON with the keys {{{}}}. A refusal is prose instead, marked as an error.",
        keys.join(", ")
    ));
    description
}

/// A tool's outcome as the `tool_result` block the model reads: [`result_text`], with
/// `is_error` carried.
pub fn tool_result(id: &str, outcome: &ToolOutcome) -> ContentBlock {
    ContentBlock::ToolResult {
        tool_use_id: id.to_string(),
        content: result_text(outcome),
        is_error: outcome.is_error,
        cache_control: None,
    }
}

/// What a tool returned, as text: a result's structured payload pretty-printed, else its
/// text. A refusal leads with its sentence, which says why; the structured part a
/// refusal carries — a refused `satz_transpile_check` hands over what the compile found
/// — follows it after a blank line.
pub fn result_text(outcome: &ToolOutcome) -> String {
    let sentence = outcome.text.trim();
    match &outcome.structured {
        None => outcome.text.clone(),
        Some(value) if !outcome.is_error || sentence.is_empty() => pretty(value),
        Some(value) => format!("{sentence}\n\n{}", pretty(value)),
    }
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).expect("a JSON value serialises")
}
