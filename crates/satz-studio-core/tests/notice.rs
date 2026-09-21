//! The NOTICE every bundle carries holds satz's NOTICE verbatim: satz-core is compiled
//! into the app, and Apache 2.0 §4(d) asks a redistribution of a work that includes it to
//! carry its attribution notices. A pin bump that changes `vendor/satz/NOTICE` fails here
//! until the repository's `NOTICE` carries the new text.

use std::path::PathBuf;

fn read(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

#[test]
fn notice_carries_the_notice_of_satz_verbatim() {
    let ours = read("NOTICE");
    let satz = read("vendor/satz/NOTICE");
    assert!(
        !satz.trim().is_empty(),
        "vendor/satz/NOTICE is empty — is the submodule checked out?"
    );
    assert!(
        ours.contains(&satz),
        "NOTICE does not contain vendor/satz/NOTICE verbatim: replace the text under \
         \"The NOTICE of satz\" in NOTICE with the current vendor/satz/NOTICE"
    );
}
