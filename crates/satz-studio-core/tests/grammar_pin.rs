//! The vendored grammar is the one the pinned satz names. satz holds its own files to the
//! grammar commit in `editors/zed/extension.toml` (`scripts/check-grammar.sh` there), so
//! that commit reads every form the pinned satz reads; a copy taken from another commit
//! reads a file of the pinned satz as an error node, or carries a node the document layer
//! does not map. Moving the pin without the grammar fails here, with what to run.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate stands two levels below the repository root")
        .to_path_buf()
}

/// `rev` of the `[grammars.satz]` table of satz's Zed extension manifest.
fn pinned_by_satz(manifest: &str) -> Option<String> {
    let mut in_table = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_table = line == "[grammars.satz]";
            continue;
        }
        if in_table && let Some(rest) = line.strip_prefix("rev") {
            let value = rest.trim_start().strip_prefix('=')?.trim();
            return Some(value.trim_matches('"').to_string());
        }
    }
    None
}

#[test]
fn the_vendored_grammar_is_the_commit_the_pinned_satz_names() {
    let root = repo_root();
    let manifest = root.join("vendor/satz/editors/zed/extension.toml");
    let text = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("{}: {e}", manifest.display()));
    let wanted = pinned_by_satz(&text)
        .unwrap_or_else(|| panic!("{}: no `rev` under [grammars.satz]", manifest.display()));
    let commit = root.join("vendor/satz-tree-sitter/COMMIT");
    let have =
        std::fs::read_to_string(&commit).unwrap_or_else(|e| panic!("{}: {e}", commit.display()));
    assert_eq!(
        have.trim(),
        wanted,
        "vendor/satz-tree-sitter is not the grammar the pinned satz names — run \
         `scripts/move-pin.sh <tag>`, or `scripts/sync-grammar.sh <checkout of the grammar \
         at {wanted}>`"
    );
}

#[test]
fn the_pin_is_read_from_the_grammar_table_alone() {
    let manifest = "id = \"satz\"\nrev = \"not-this\"\n\n[grammars.satz]\nrepository = \"https://example.com/g\"\nrev = \"abc123\"\n\n[other]\nrev = \"nor-this\"\n";
    assert_eq!(pinned_by_satz(manifest).as_deref(), Some("abc123"));
    assert_eq!(
        pinned_by_satz("[grammars.satz]\nrepository = \"x\"\n"),
        None
    );
}
