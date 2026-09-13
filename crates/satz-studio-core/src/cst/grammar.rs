//! The tree-sitter language for Satz, compiled from the vendored generated parser
//! (`vendor/satz-tree-sitter/src/parser.c`, commit in `vendor/satz-tree-sitter/COMMIT`).

unsafe extern "C" {
    fn tree_sitter_satz() -> *const tree_sitter::ffi::TSLanguage;
}

/// The Satz grammar.
pub fn language() -> tree_sitter::Language {
    // SAFETY: `tree_sitter_satz` is the symbol the generated parser exports; it returns a
    // pointer to a static TSLanguage that lives for the whole program.
    unsafe { tree_sitter::Language::from_raw(tree_sitter_satz()) }
}

/// A parser for Satz, ready to use.
pub fn parser() -> tree_sitter::Parser {
    let mut p = tree_sitter::Parser::new();
    p.set_language(&language()).expect("the vendored grammar matches the tree-sitter crate's ABI");
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vendored_grammar_parses_an_estate_without_errors() {
        let src = "// header\nestate showcase\n\nparams {\n  a = 1   # trailing\n  b = \"{a}-x\"\n}\n\n// use \"presets/x.satz\" when use_x\ngoogle_folder infra {\n  display_name = \"Infrastructure\"\n}\n";
        let tree = parser().parse(src, None).unwrap();
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(!root.has_error(), "{}", root.to_sexp());
        // comments are nodes, so nothing is lost
        let mut cursor = root.walk();
        let comments = root.children(&mut cursor).filter(|n| n.kind() == "comment").count();
        assert_eq!(comments, 2);
    }

    #[test]
    fn the_grammar_and_satz_core_agree_on_the_smoke_estates() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/satz/tests/smoke/yaml");
        let mut p = parser();
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("satz") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            let tree = p.parse(&src, None).unwrap();
            assert!(!tree.root_node().has_error(), "{}: {}", path.display(), tree.root_node().to_sexp());
            satz_core::satz::parse(&src).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        }
    }
}
