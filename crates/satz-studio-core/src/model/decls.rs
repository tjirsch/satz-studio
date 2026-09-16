//! What the files an estate reads declare between its choices, read with satz-core's
//! parser and kept in no table of the app's own: every question that waits on a gate
//! (`ask_when`), and whether the binding that applies to its subject is that gate by
//! reference.
//!
//! The files are the estate itself, every pack its `use` lines name — active or
//! commented, whatever the gate says — and every file those name in turn. A pack that is
//! off still declares what it waits on, so the dependency is known before the pack is
//! switched on, and the tree the Packs view draws does not rearrange when a switch flips.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use satz_core::satz::{Entry, File, Value};

use crate::cst::{Cst, scan_uses};

/// The declarations [`super::EstateModel::build`] derives the pack edges from.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PackDecls {
    /// every question with an `ask_when`, in the order the files were read
    pub asks: Vec<AskWhen>,
    /// the files that could not be loaded or parsed, and why
    pub unread: Vec<Unread>,
}

/// `question <subject> { … ask_when = <when> }`.
#[derive(Debug, Clone, PartialEq)]
pub struct AskWhen {
    /// the param the question answers, or the group name of a `oneof`
    pub subject: String,
    pub oneof: bool,
    /// the params a `oneof` chooses between; empty for a param question
    pub options: Vec<String>,
    /// the param the question waits on
    pub when: String,
    /// the binding that applies to `subject` — the estate's own when it binds one, else
    /// the declaring file's default — is the bare reference `when`: the subject is on
    /// while its gate is, until somebody answers it. Never for a `oneof`.
    pub follows: bool,
    /// the declaring file: the estate's path, or the path a `use` line names
    pub file: String,
    pub line: usize,
}

/// A file named by a `use` line that satz-core's parser did not get to read.
#[derive(Debug, Clone, PartialEq)]
pub struct Unread {
    /// the path as the `use` line names it, or the estate's own path
    pub path: String,
    /// the estate file's line naming it; `None` for a file a pack names
    pub line: Option<u32>,
    pub why: String,
}

impl PackDecls {
    /// Read the estate `cst` (at `main`) and every file its `use` lines reach, each once,
    /// through `load` — the loader satz's pipeline resolves `use "…"` with.
    pub fn read(
        main: &Path,
        cst: &Cst,
        load: &dyn Fn(&str) -> Result<String, String>,
    ) -> PackDecls {
        let mut out = PackDecls::default();
        let estate = match cst.lower() {
            Ok(file) => file,
            Err(e) => {
                out.unread.push(Unread {
                    path: main.display().to_string(),
                    line: Some(e.line as u32),
                    why: e.msg,
                });
                return out;
            }
        };
        let own: BTreeMap<String, Value> = estate
            .params
            .iter()
            .map(|(name, value, _)| (name.clone(), value.clone()))
            .collect();
        collect(&estate, &main.display().to_string(), &own, &mut out);

        let mut queue: VecDeque<(String, Option<u32>)> = scan_uses(cst)
            .into_iter()
            .map(|u| (u.path, Some(u.line)))
            .collect();
        let mut seen = BTreeSet::new();
        while let Some((path, line)) = queue.pop_front() {
            if !seen.insert(path.clone()) {
                continue;
            }
            let parsed = load(&path).and_then(|src| {
                satz_core::satz::parse(&src).map_err(|e| format!("line {}: {}", e.line, e.msg))
            });
            let file = match parsed {
                Ok(file) => file,
                Err(why) => {
                    out.unread.push(Unread { path, line, why });
                    continue;
                }
            };
            collect(&file, &path, &own, &mut out);
            let mut nested = Vec::new();
            uses_in(&file.items, &mut nested);
            queue.extend(nested.into_iter().map(|p| (p, None)));
        }
        out
    }
}

/// The questions of one file.
fn collect(file: &File, name: &str, own: &BTreeMap<String, Value>, out: &mut PackDecls) {
    let defaults: BTreeMap<&str, &Value> = file
        .params
        .iter()
        .map(|(n, v, _)| (n.as_str(), v))
        .collect();
    for q in &file.questions {
        let Some(when) = &q.ask_when else {
            continue;
        };
        let applies = own
            .get(&q.subject)
            .or_else(|| defaults.get(q.subject.as_str()).copied());
        out.asks.push(AskWhen {
            subject: q.subject.clone(),
            oneof: q.oneof,
            options: q.options.iter().map(|o| o.param.clone()).collect(),
            when: when.clone(),
            follows: !q.oneof && matches!(applies, Some(Value::Ref(r)) if r == when),
            file: name.to_string(),
            line: q.line,
        });
    }
}

/// Every `use` path of a parsed file, blocks included, in document order.
fn uses_in(items: &[Entry], out: &mut Vec<String>) {
    for item in items {
        match item {
            Entry::Use { path, .. } => out.push(path.clone()),
            Entry::Map { body, .. } => uses_in(body, out),
            Entry::Attr { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(files: &[(&str, &str)]) -> impl Fn(&str) -> Result<String, String> {
        let files: BTreeMap<String, String> = files
            .iter()
            .map(|(p, t)| (p.to_string(), t.to_string()))
            .collect();
        move |p: &str| {
            files
                .get(p)
                .cloned()
                .ok_or_else(|| format!("use \"{p}\": file not found"))
        }
    }

    const MAP: &str = concat!(
        "pack m version \"1\"\n\n",
        "params {\n  use_a = false\n  use_b = false\n  use_c = use_a\n  pick_x = false\n  pick_y = false\n}\n\n",
        "question use_b {\n  prompt = \"B?\"\n  reversal = edit\n  blast = none\n  ask_when = use_a\n}\n\n",
        "question use_c {\n  prompt = \"C?\"\n  reversal = edit\n  blast = none\n  ask_when = use_a\n}\n\n",
        "question oneof pick {\n  prompt = \"Which?\"\n  reversal = edit\n  blast = none\n  required = true\n  ask_when = use_b\n",
        "  option pick_x { label = \"X\" }\n  option pick_y { label = \"Y\" }\n}\n",
    );

    #[test]
    fn every_ask_when_of_a_commented_pack_is_read_with_whether_it_follows_its_gate() {
        let cst = Cst::parse("estate acme\n\n// use \"m.satz\"\n").unwrap();
        let d = PackDecls::read(Path::new("acme.satz"), &cst, &loader(&[("m.satz", MAP)]));
        assert!(d.unread.is_empty(), "{:?}", d.unread);
        let asks: Vec<(&str, &str, bool, bool)> = d
            .asks
            .iter()
            .map(|a| (a.subject.as_str(), a.when.as_str(), a.oneof, a.follows))
            .collect();
        assert_eq!(
            asks,
            [
                ("use_b", "use_a", false, false),
                ("use_c", "use_a", false, true),
                ("pick", "use_b", true, false),
            ]
        );
        assert_eq!(d.asks[2].options, ["pick_x", "pick_y"]);
    }

    #[test]
    fn the_estate_s_own_literal_binding_is_what_applies_so_the_child_no_longer_follows() {
        let cst =
            Cst::parse("estate acme\n\nparams {\n  use_c = true\n}\n\nuse \"m.satz\"\n").unwrap();
        let d = PackDecls::read(Path::new("acme.satz"), &cst, &loader(&[("m.satz", MAP)]));
        let c = d.asks.iter().find(|a| a.subject == "use_c").unwrap();
        assert!(!c.follows);
    }

    #[test]
    fn a_file_named_twice_is_read_once_and_one_that_does_not_load_is_named_with_its_line() {
        let cst = Cst::parse(
            "estate acme\n\nuse \"m.satz\"\n// use \"m.satz\" when use_a\n// use \"gone.satz\" when use_b\n",
        )
        .unwrap();
        let d = PackDecls::read(Path::new("acme.satz"), &cst, &loader(&[("m.satz", MAP)]));
        assert_eq!(d.asks.len(), 3);
        assert_eq!(d.unread.len(), 1);
        assert_eq!(d.unread[0].path, "gone.satz");
        assert_eq!(d.unread[0].line, Some(5));
        assert!(d.unread[0].why.contains("not found"), "{}", d.unread[0].why);
    }

    #[test]
    fn a_pack_s_own_use_lines_are_followed() {
        let cst = Cst::parse("estate acme\n\nuse \"outer.satz\"\n").unwrap();
        let outer = "pack outer version \"1\"\n\nuse \"m.satz\"\n";
        let d = PackDecls::read(
            Path::new("acme.satz"),
            &cst,
            &loader(&[("outer.satz", outer), ("m.satz", MAP)]),
        );
        assert!(d.unread.is_empty(), "{:?}", d.unread);
        assert_eq!(d.asks.len(), 3);
        assert!(d.asks.iter().all(|a| a.file == "m.satz"));
    }
}
