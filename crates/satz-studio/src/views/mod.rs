//! The views. Each is one component the shell renders for its [`crate::state::View`].
//! The two helpers below are what the estate views share: a value's source text and a
//! line's text, sliced from the document tree the reload keeps in the store.

pub mod agent;
pub mod checks;
pub mod commands;
pub mod create;
pub mod deploy;
pub mod estate;
pub mod estates;
pub mod export;
pub mod gallery;
pub mod import;
pub mod interview;
pub mod map;
pub mod overview;
pub mod params;
pub mod resources;
pub mod review;
pub mod satz_release;
pub mod settings;

use satz_studio_core::cst::{Cst, NodeId, NodeKind};

/// The Satz source of a value: the value node's own text, or the value of the
/// attribute or param entry `id` names. `None` for a node that has no value.
pub fn value_source(cst: &Cst, id: NodeId) -> Option<String> {
    let node = cst.node(id);
    let target = match &node.kind {
        NodeKind::Attr { value, .. } | NodeKind::ParamEntry { value, .. } => *value,
        NodeKind::Value(_) => id,
        _ => return None,
    };
    Some(cst.slice(cst.node(target).span).to_string())
}

/// Line `line` of the file, 1-based as satz counts, without its newline.
pub fn line_text(cst: &Cst, line: u32) -> String {
    cst.text()
        .lines()
        .nth(line.saturating_sub(1) as usize)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_param_entry_and_an_attribute_slice_their_value_and_a_block_has_none() {
        let cst = Cst::parse(
            "estate e\n\nparams {\n  region = \"europe-west3\"\n}\n\ngoogle_folder {\n  x {\n    display_name = [ \"a\",\n      \"b\" ]\n  }\n}\n",
        )
        .unwrap();
        let entry = cst.param("region").unwrap();
        assert_eq!(
            value_source(&cst, entry).as_deref(),
            Some("\"europe-west3\"")
        );
        let attr = cst
            .nodes()
            .find(|(_, n)| matches!(n.kind, NodeKind::Attr { .. }))
            .map(|(id, _)| id)
            .unwrap();
        assert_eq!(
            value_source(&cst, attr).as_deref(),
            Some("[ \"a\",\n      \"b\" ]")
        );
        let block = cst
            .nodes()
            .find(|(_, n)| matches!(n.kind, NodeKind::Block { .. }))
            .map(|(id, _)| id)
            .unwrap();
        assert_eq!(value_source(&cst, block), None);
        assert_eq!(line_text(&cst, 4), "  region = \"europe-west3\"");
        assert_eq!(line_text(&cst, 99), "");
    }
}
