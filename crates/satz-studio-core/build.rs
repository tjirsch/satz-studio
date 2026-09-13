//! Compiles the generated tree-sitter parser for Satz (`vendor/satz-tree-sitter/src`,
//! copied from the grammar repository at the commit named in
//! `vendor/satz-tree-sitter/COMMIT`). The grammar repository is private, so its
//! generated `src/` is vendored rather than pulled as a submodule; `scripts/sync-grammar.sh`
//! refreshes it.

use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/satz-tree-sitter/src");
    let parser = root.join("parser.c");
    println!("cargo:rerun-if-changed={}", parser.display());
    println!("cargo:rerun-if-changed={}", root.join("tree_sitter/parser.h").display());
    let scanner = root.join("scanner.c");
    let mut build = cc::Build::new();
    build.include(&root).file(&parser).warnings(false);
    if scanner.exists() {
        println!("cargo:rerun-if-changed={}", scanner.display());
        build.file(&scanner);
    }
    build.compile("tree-sitter-satz");
}
