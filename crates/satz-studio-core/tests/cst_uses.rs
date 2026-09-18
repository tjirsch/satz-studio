//! `scan_uses` over the skeleton `satz interview --create` writes — every pack line,
//! commented or active, with its phase comment — and over hand-written shapes at the
//! edges of what a pack line is.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use satz_studio_core::cst::{Cst, UseState, scan_uses};

fn vendor() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/satz")
        .canonicalize()
        .expect("vendor/satz")
}

/// The installed satz: on `PATH`, else where its installer puts it.
fn satz_binary() -> PathBuf {
    if let Ok(p) = which::which("satz") {
        return p;
    }
    let local = dirs::home_dir()
        .expect("a home directory")
        .join(".local/bin/satz");
    assert!(
        local.exists(),
        "satz is neither on PATH nor at {}",
        local.display()
    );
    local
}

#[test]
fn the_interview_skeleton_is_scanned_line_for_line() {
    let vendor = vendor();
    let tmp = tempfile::tempdir().unwrap();
    let yaml = tmp.path().join("yaml");
    fs::create_dir_all(&yaml).unwrap();
    let config = format!(
        "yaml_dir = {yaml:?}\nhcl_dir = \"hcl\"\ninclude_dirs = [\".\", {yaml:?}, {vendor:?}]\npresets_dir = {presets:?}\nschema_dir = {schemas:?}\ntf_tool = \"tofu\"\nprovider_version = \"7.14.1\"\n",
        presets = vendor.join("presets"),
        schemas = vendor.join("tests/schemas"),
    );
    fs::write(tmp.path().join("config.toml"), config).unwrap();
    let estate = yaml.join("new.satz");
    let out = Command::new(satz_binary())
        .arg("--config")
        .arg(tmp.path())
        .arg("interview")
        .arg(&estate)
        .arg("--create")
        .stdin(Stdio::null())
        .output()
        .expect("run satz");
    assert!(
        out.status.success(),
        "satz interview --create failed ({}):\n{}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let text = fs::read_to_string(&estate).unwrap();
    let cst = Cst::parse(&text).unwrap();
    let uses = scan_uses(&cst);
    let lines: Vec<&str> = text.lines().collect();

    for u in &uses {
        assert_eq!(
            cst.slice(u.span),
            lines[u.line as usize - 1],
            "the span is the whole line {}",
            u.line
        );
        assert!(!u.path.is_empty(), "line {}: no path", u.line);
    }
    assert!(
        uses.windows(2).all(|w| w[0].line < w[1].line),
        "not in document order"
    );

    let active: Vec<_> = uses
        .iter()
        .filter(|u| u.state == UseState::Active)
        .collect();
    assert_eq!(active.len(), 1, "{active:?}");
    assert_eq!(active[0].path, "presets/estate-core.satz");
    assert_eq!(active[0].gate, None);
    assert!(
        active[0]
            .phase_comment
            .as_deref()
            .is_some_and(|c| c.starts_with("The day-0 params")),
        "{:?}",
        active[0].phase_comment
    );

    let commented: Vec<_> = uses
        .iter()
        .filter(|u| u.state == UseState::Commented)
        .collect();
    let written = text
        .lines()
        .filter(|l| l.trim_start().starts_with("// use \""))
        .count();
    assert_eq!(
        commented.len(),
        written,
        "every commented pack line is scanned"
    );
    for u in &commented {
        // The map is the one line no question gates. The CIS baseline was the other
        // until satz v0.64.0 (ADR 0028) gave it `use_cis_baseline` and moved it under
        // `presets/cis/`.
        let ungated = ["presets/estate-map.satz"];
        assert_eq!(
            u.gate.is_none(),
            ungated.contains(&u.path.as_str()),
            "line {}: {:?}",
            u.line,
            u
        );
        assert!(u.as_key.is_none(), "line {}: satz writes no `as`", u.line);
    }

    let map = commented
        .iter()
        .position(|u| u.path == "presets/estate-map.satz")
        .expect("the map line");
    let map_caption = commented[map]
        .phase_comment
        .as_deref()
        .expect("the map line's caption");
    assert!(
        map_caption.starts_with("once the estate runs as the service account"),
        "{map_caption}"
    );
    assert!(
        !map_caption.contains("Uncomment a line"),
        "the menu header is across a blank line: {map_caption}"
    );
    let s1 = commented[map + 1];
    assert!(s1.path.ends_with("s1-security-groups.satz"), "{s1:?}");
    assert_eq!(s1.gate.as_deref(), Some("security_model_s1"));
    let caption = s1.phase_comment.as_deref().expect("s1 has a caption");
    assert!(caption.contains("map"), "{caption}");
    let s2 = commented[map + 2];
    assert!(s2.path.ends_with("s2-security-groups.satz"), "{s2:?}");
    assert_eq!(
        s2.phase_comment, None,
        "a pack line directly under a pack line has no caption"
    );

    for (path, gate) in [
        (
            "presets/monitoring/organization-audit-logsink.satz",
            "use_audit_logsink",
        ),
        (
            "presets/monitoring/organization-cis-log-alerts-central.satz",
            "use_central_alerts",
        ),
    ] {
        let u = commented
            .iter()
            .find(|u| u.path == path)
            .unwrap_or_else(|| panic!("{path} not found"));
        assert_eq!(u.gate.as_deref(), Some(gate));
        let line = cst.slice(u.span);
        assert!(
            line.starts_with("    // use \""),
            "indentation kept: {line:?}"
        );
        assert_eq!(line.trim_start(), format!("// use \"{path}\" when {gate}"));
        assert!(u.phase_comment.is_some(), "{path} has a caption");
    }
}

#[test]
fn only_the_exact_shape_is_a_pack_line() {
    let text = "// caption one\n// caption two\n// use \"a.satz\"\n// use \"b.satz\" when g\n\n// use \"c.satz\" when g extra\n//use \"d.satz\"\n# use \"e.satz\"\n// use \"f.satz\" as k when g   \n/* block */\n// use \"h.satz\" when h\nx {\n  y = 1 // trailing\n  // indented caption\n  use \"i.satz\" when i\n}\n";
    let cst = Cst::parse(text).unwrap();
    let uses = scan_uses(&cst);
    let paths: Vec<&str> = uses.iter().map(|u| u.path.as_str()).collect();
    assert_eq!(paths, ["a.satz", "b.satz", "f.satz", "h.satz", "i.satz"]);

    let a = &uses[0];
    assert_eq!(
        (a.state, a.gate.as_deref(), a.line),
        (UseState::Commented, None, 3)
    );
    assert_eq!(a.phase_comment.as_deref(), Some("caption one\ncaption two"));
    assert_eq!(cst.slice(a.span), "// use \"a.satz\"");

    let b = &uses[1];
    assert_eq!(b.gate.as_deref(), Some("g"));
    assert_eq!(b.phase_comment, None, "directly under a pack line");

    let f = &uses[2];
    assert_eq!(
        (f.as_key.as_deref(), f.gate.as_deref()),
        (Some("k"), Some("g"))
    );
    assert_eq!(
        cst.slice(f.span),
        "// use \"f.satz\" as k when g   ",
        "the line keeps its trailing whitespace"
    );
    assert_eq!(
        f.phase_comment.as_deref(),
        Some("use \"c.satz\" when g extra\nuse \"d.satz\"\nuse \"e.satz\""),
        "the near-misses above are plain comments, and the run ends at the blank line"
    );

    let h = &uses[3];
    assert_eq!(h.phase_comment, None, "a block comment carries no caption");

    let i = &uses[4];
    assert_eq!(
        (i.state, i.gate.as_deref(), i.line),
        (UseState::Active, Some("i"), 15)
    );
    assert_eq!(cst.slice(i.span), "  use \"i.satz\" when i");
    assert_eq!(i.phase_comment.as_deref(), Some("indented caption"));
}

#[test]
fn an_active_use_with_as_and_a_triple_quoted_path() {
    let cst =
        Cst::parse("use \"\"\"p.satz\"\"\" as google_org_policy_policy\nuse \"q.satz\"\n").unwrap();
    let uses = scan_uses(&cst);
    assert_eq!(uses.len(), 2);
    assert_eq!(uses[0].path, "p.satz");
    assert_eq!(uses[0].as_key.as_deref(), Some("google_org_policy_policy"));
    assert_eq!(uses[1].path, "q.satz");
    assert_eq!(
        uses[1].phase_comment, None,
        "an active use directly above is not a caption"
    );
}
