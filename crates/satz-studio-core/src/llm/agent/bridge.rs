//! The tool bridge: MCP tools as Claude tool definitions, and results back.

use super::super::claude::types::{CacheControl, ContentBlock, ToolDef};
use crate::satz::{ToolInfo, ToolOutcome};

/// Every MCP tool as a [`ToolDef`]: the name and `input_schema` verbatim, the
/// description followed by `Returns: {a, b, c}` — the top-level keys of the output
/// schema — when the tool has one. Sorted by name for a stable cache prefix, the cache
/// breakpoint on the last.
pub fn tool_defs(tools: &[ToolInfo]) -> Vec<ToolDef> {
    let mut defs: Vec<ToolDef> = tools.iter().map(|t| ToolDef { name: t.name.clone(), description: describe(t), input_schema: t.input_schema.clone(), cache_control: None }).collect();
    defs.sort_by(|a, b| a.name.cmp(&b.name));
    if let Some(last) = defs.last_mut() {
        last.cache_control = Some(CacheControl::ephemeral());
    }
    defs
}

fn describe(tool: &ToolInfo) -> String {
    let mut description = tool.description.trim().to_string();
    let keys: Vec<&str> = tool.output_schema.as_ref().and_then(|s| s.get("properties")).and_then(serde_json::Value::as_object).map(|p| p.keys().map(String::as_str).collect()).unwrap_or_default();
    if keys.is_empty() {
        return description;
    }
    if !description.is_empty() && !description.ends_with('.') {
        description.push('.');
    }
    if !description.is_empty() {
        description.push(' ');
    }
    description.push_str(&format!("Returns: {{{}}}", keys.join(", ")));
    description
}

/// A tool's outcome as the `tool_result` block the model reads: the structured payload
/// pretty-printed when there is one, else the text; `is_error` carried.
pub fn tool_result(id: &str, outcome: &ToolOutcome) -> ContentBlock {
    let content = match &outcome.structured {
        Some(value) => serde_json::to_string_pretty(value).expect("a JSON value serialises"),
        None => outcome.text.clone(),
    };
    ContentBlock::ToolResult { tool_use_id: id.to_string(), content, is_error: outcome.is_error, cache_control: None }
}
