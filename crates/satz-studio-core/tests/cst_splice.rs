//! A value edit is a splice into the value's span: the result parses, and splicing the
//! old bytes back is the identity.

use std::sync::OnceLock;

use proptest::prelude::*;
use satz_studio_core::cst::{Cst, NodeId, NodeKind, TypedValue, render_value, style_of};

const FILES: [&str; 2] = ["showcase.satz", "smoke.satz"];

fn text_of(name: &str) -> &'static str {
    static TEXTS: OnceLock<Vec<String>> = OnceLock::new();
    let texts = TEXTS.get_or_init(|| {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../vendor/satz/tests/smoke/yaml");
        FILES
            .iter()
            .map(|f| std::fs::read_to_string(dir.join(f)).unwrap_or_else(|e| panic!("{f}: {e}")))
            .collect()
    });
    &texts[FILES.iter().position(|f| *f == name).unwrap()]
}

/// The value node of every attribute and param.
fn targets(cst: &Cst) -> Vec<NodeId> {
    cst.nodes()
        .filter_map(|(_, n)| match &n.kind {
            NodeKind::Attr { value, .. } | NodeKind::ParamEntry { value, .. } => Some(*value),
            _ => None,
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(160))]
    #[test]
    fn a_string_spliced_into_any_value_parses_and_splices_back(
        which in 0usize..FILES.len(),
        pick in any::<prop::sample::Index>(),
        s in "[^\\x00]{0,40}",
    ) {
        let name = FILES[which];
        let text = text_of(name);
        let cst = Cst::parse(text).unwrap();
        let targets = targets(&cst);
        prop_assert!(targets.len() > 20, "{name}: {} targets", targets.len());
        let node = targets[pick.index(targets.len())];
        let span = cst.node(node).span;
        let rendered = render_value(&TypedValue::Str(s.clone()), &style_of(&cst, node));
        let edited = format!("{}{}{}", &text[..span.start], rendered, &text[span.end..]);
        if let Err(e) = satz_core::satz::parse(&edited) {
            prop_assert!(false, "{name} line {}: {:?} spliced over {:?} does not parse: {e}", cst.node(node).line, rendered, cst.slice(span));
        }
        let after = Cst::parse(&edited).unwrap();
        prop_assert!(
            !after.nodes().any(|(_, n)| matches!(n.kind, NodeKind::Error { .. })),
            "{name} line {}: the grammar refuses {:?}", cst.node(node).line, rendered
        );
        let reverted = format!("{}{}{}", &edited[..span.start], cst.slice(span), &edited[span.start + rendered.len()..]);
        prop_assert_eq!(reverted, text);
    }
}
