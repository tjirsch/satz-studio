//! `satz review-pack` and the two places a reviewed pack goes.
//!
//! The rules a pack is held to are satz's (`vendor/satz/src/review_pack.rs`): the app
//! runs the command, types its report ([`PackReview`]) and shows each finding at its
//! line. It runs the CLI, `satz --config <estate dir> review-pack <pack> --format json`,
//! and not the `satz_review_pack` tool: `satz mcp` confines every path to the estate's
//! root, and a pack under review is usually a file its author keeps elsewhere — placing
//! it in the estate's library is the second destination below, which comes after the
//! review.
//!
//! The review holds the bytes it judged. A pack that changes during the review is
//! refused, and a pack that changed since is refused by the placement, so what lands is
//! what satz looked at.
//!
//! Two destinations:
//!
//! - **upstream**, a pull request to the satz repository, by hand: the pack goes in
//!   under `presets/` as [`upstream_name`], clean in the review, with a row in the
//!   library's changelog, and through that repository's privacy gate. Nothing here
//!   automates it.
//! - **private**, [`place_private`]: the pack copied into the estate's `presets_dir` as
//!   `<stem>.local.satz` ([`local_name`]), the suffix satz's updates never touch.

use std::io::Write;
use std::path::{Path, PathBuf};

use super::reports::PackReview;
use super::{SatzCli, SatzError};
use crate::diag::{DiagSource, Diagnostic, plain};
use crate::edit::{CheckFailure, Checker, sha256_hex};
use crate::satz::reports::CompileSummary;

/// The name the findings of a review carry in the drawer: `satz review-pack`.
pub const COMMAND: &str = "review-pack";

/// A pack, the bytes the review judged, and what satz said about them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewedPack {
    /// canonical, in the form satz names it by
    pub path: PathBuf,
    /// the pack's text as the review read it
    pub text: String,
    pub sha256: String,
    /// judged inside the open estate (`--against`) rather than a synthesised one
    pub against: bool,
    pub review: PackReview,
}

impl ReviewedPack {
    /// Every finding as a diagnostic at its `file:line`, from `satz review-pack`.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        diagnostics(&self.path, &self.review)
    }

    /// The pack is the library's own `<stem>.local.satz` already — the file
    /// [`place_private`] would write — compared in the canonical form the review keeps.
    pub fn is_in_library(&self, presets_dir: &Path) -> bool {
        local_target(presets_dir, &self.path)
            .ok()
            .and_then(|t| std::fs::canonicalize(t).ok())
            .is_some_and(|t| plain(&t) == self.path)
    }
}

/// `review-pack <pack> [--against <estate>]`: the arguments before the format and the
/// destination, which [`SatzCli::json_verdict`] adds.
pub fn review_args(pack: &Path, against: Option<&Path>) -> Vec<String> {
    let mut args = vec![COMMAND.to_string(), pack.display().to_string()];
    if let Some(estate) = against {
        args.push("--against".to_string());
        args.push(estate.display().to_string());
    }
    args
}

/// Review `pack` with the estate's config: its `presets_dir` is the library whose
/// changelog and adoption rules the pack is held to, its `schema_dir` the provider
/// schema the fold compiles against. `against` judges the pack inside that estate file
/// instead of a synthesised one.
///
/// Refused: a pack that is not UTF-8 (satz reads no other), a pack that changed while
/// satz read it, a report satz did not write, and an exit status that contradicts the
/// report's own verdict.
pub async fn review(
    cli: &SatzCli,
    pack: &Path,
    against: Option<&Path>,
) -> Result<ReviewedPack, SatzError> {
    // satz names the pack by its canonical path in every finding, so the review keeps
    // that form: a pack reached through a symlinked directory (`/tmp` on macOS) is one
    // file to the drawer, the view and satz alike
    let path = std::fs::canonicalize(pack)
        .map(|p| plain(&p))
        .map_err(|e| SatzError::Io {
            context: format!("resolving {}", pack.display()),
            source: e,
        })?;
    let before = read_text(&path)?;
    let args = review_args(&path, against);
    let (review, status): (PackReview, _) = cli.json_verdict(&args).await?;
    if status.success() != review.passed() {
        return Err(SatzError::Verdict {
            command: args.join(" "),
            status,
            report: if review.passed() {
                "the pack clears the bar".to_string()
            } else {
                "the pack does not clear the bar".to_string()
            },
        });
    }
    let after = read_text(&path)?;
    if after != before {
        return Err(SatzError::Io {
            context: format!("reviewing {}", path.display()),
            source: std::io::Error::other("the file changed while satz read it — review it again"),
        });
    }
    Ok(ReviewedPack {
        sha256: sha256_hex(before.as_bytes()),
        path,
        text: before,
        against: against.is_some(),
        review,
    })
}

fn read_text(path: &Path) -> Result<String, SatzError> {
    let bytes = std::fs::read(path).map_err(|e| SatzError::Io {
        context: format!("reading {}", path.display()),
        source: e,
    })?;
    String::from_utf8(bytes).map_err(|e| SatzError::Io {
        context: format!("reading {}", path.display()),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
    })
}

/// Each finding at the file and line it names. satz names the pack by its absolute path;
/// a relative one resolves against the pack's directory.
pub fn diagnostics(pack: &Path, review: &PackReview) -> Vec<Diagnostic> {
    let base = pack.parent().unwrap_or(Path::new("."));
    review
        .findings
        .iter()
        .map(|f| Diagnostic::from_finding(base, f, DiagSource::Command(COMMAND.to_string())))
        .collect()
}

/// Why a pack was not placed. In every case the library is as it was.
#[derive(Debug, thiserror::Error)]
pub enum PlaceError {
    #[error("{0}: not a `.satz` file — satz reads no other")]
    NotSatz(PathBuf),
    #[error(
        "{0}: a `.diff.satz` is the adoption delta merge-presets writes beside a fork, not a pack"
    )]
    DiffFile(PathBuf),
    #[error("{0}: changed since the review — review it again, so what lands is what was judged")]
    ChangedSinceReview(PathBuf),
    #[error("{0}: no such directory — the estate's presets_dir; `satz get-presets` writes it")]
    NoLibrary(PathBuf),
    #[error(
        "{0} exists and holds other text — it is the estate's own fork, and the app never writes over it. Rename the pack, or merge the two by hand"
    )]
    Exists(PathBuf),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// the estate did not compile with the pack in the library; the file is gone again
    #[error("not placed: the estate did not compile with it in the library")]
    Rollback(Vec<Diagnostic>),
    #[error("not placed: {0}")]
    Satz(SatzError),
}

/// What a placement did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placed {
    /// the file is new in the library, and the estate compiles with it there
    Written {
        path: PathBuf,
        summary: CompileSummary,
    },
    /// the library already holds these very bytes under that name: nothing was written
    AlreadyThere(PathBuf),
}

/// The name a private pack takes in the library: `<stem>.local.satz`. A pack that is
/// already a `.local.satz` keeps its name; a `.diff.satz` is no pack.
pub fn local_name(pack: &Path) -> Result<String, PlaceError> {
    if pack.extension().and_then(|e| e.to_str()) != Some("satz") {
        return Err(PlaceError::NotSatz(pack.to_path_buf()));
    }
    let stem = pack
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or_else(|| PlaceError::NotSatz(pack.to_path_buf()))?;
    if stem.ends_with(".diff") {
        return Err(PlaceError::DiffFile(pack.to_path_buf()));
    }
    let base = stem.strip_suffix(".local").unwrap_or(&stem);
    if base.is_empty() {
        return Err(PlaceError::NotSatz(pack.to_path_buf()));
    }
    Ok(format!("{base}.local.satz"))
}

/// Where [`place_private`] writes: [`local_name`] at the top of `presets_dir`.
pub fn local_target(presets_dir: &Path, pack: &Path) -> Result<PathBuf, PlaceError> {
    Ok(presets_dir.join(local_name(pack)?))
}

/// The name the pack goes upstream under: its stem without `.local`, as a pristine
/// `presets/<stem>.satz` of the library. The folder inside `presets/` is the library's
/// call, made in the pull request.
pub fn upstream_name(pack: &Path) -> Result<String, PlaceError> {
    let local = local_name(pack)?;
    let base = local
        .strip_suffix(".local.satz")
        .expect("local_name ends so");
    Ok(format!("presets/{base}.satz"))
}

/// Destination B: the reviewed bytes into the estate's library as `<stem>.local.satz`.
///
/// 1. the pack on disk must still be the bytes the review judged
///    ([`PlaceError::ChangedSinceReview`]);
/// 2. a file of that name that holds these bytes already is [`Placed::AlreadyThere`] and
///    nothing is written; one that holds anything else is [`PlaceError::Exists`] — it is
///    the estate's own fork, never written over;
/// 3. the file is created — `create_new`, so one that appeared in the meantime is refused
///    as well — and the estate is checked with it in the library; a refusal, or a checker
///    that could not run, removes it again.
///
/// `presets_dir` must exist: an estate without a library has nowhere to put a pack
/// ([`PlaceError::NoLibrary`]).
pub async fn place_private(
    reviewed: &ReviewedPack,
    presets_dir: &Path,
    estate: &Path,
    checker: &dyn Checker,
) -> Result<Placed, PlaceError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| PlaceError::Io { path, source }
    };
    let now = std::fs::read(&reviewed.path).map_err(io(&reviewed.path))?;
    if sha256_hex(&now) != reviewed.sha256 {
        return Err(PlaceError::ChangedSinceReview(reviewed.path.clone()));
    }
    if !presets_dir.is_dir() {
        return Err(PlaceError::NoLibrary(presets_dir.to_path_buf()));
    }
    let target = local_target(presets_dir, &reviewed.path)?;
    match std::fs::read(&target) {
        Ok(there) if there == now => return Ok(Placed::AlreadyThere(target)),
        Ok(_) => return Err(PlaceError::Exists(target)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(&target)(e)),
    }
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(PlaceError::Exists(target));
        }
        Err(e) => return Err(io(&target)(e)),
    };
    if let Err(e) = file.write_all(&now).and_then(|()| file.sync_all()) {
        drop(file);
        return Err(discard(&target, io(&target)(e)));
    }
    drop(file);
    match checker.check(estate).await {
        Ok(summary) => Ok(Placed::Written {
            path: target,
            summary,
        }),
        Err(CheckFailure::Refused(diags)) => Err(discard(&target, PlaceError::Rollback(diags))),
        Err(CheckFailure::Failed(e)) => Err(discard(&target, PlaceError::Satz(e))),
    }
}

/// Remove the file a placement wrote after `first` went wrong; when even that fails, the
/// error carries both.
fn discard(path: &Path, first: PlaceError) -> PlaceError {
    match std::fs::remove_file(path) {
        Ok(()) => first,
        Err(e) => PlaceError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::other(format!(
                "{first}; and the file could not be removed: {e}"
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Severity;

    const BROKEN: &str = include_str!("../../tests/fixtures/review/team-access.json");

    #[test]
    fn the_arguments_name_the_pack_and_the_estate_it_is_judged_in() {
        assert_eq!(
            review_args(Path::new("/e/packs/team-access.satz"), None),
            ["review-pack", "/e/packs/team-access.satz"]
        );
        assert_eq!(
            review_args(
                Path::new("/e/packs/team-access.satz"),
                Some(Path::new("/e/yaml/C0example.satz"))
            ),
            [
                "review-pack",
                "/e/packs/team-access.satz",
                "--against",
                "/e/yaml/C0example.satz"
            ]
        );
    }

    /// Every finding of the recorded review is a diagnostic in the pack's file, at the
    /// line satz named where it named one, from `satz review-pack`.
    #[test]
    fn the_findings_are_diagnostics_at_file_and_line() {
        let review: PackReview = serde_json::from_str(BROKEN).unwrap();
        let pack = Path::new("/e/packs/team-access.satz");
        let diags = diagnostics(pack, &review);
        assert_eq!(diags.len(), review.findings.len());
        for (d, f) in diags.iter().zip(&review.findings) {
            assert_eq!(d.file.as_deref(), Some(pack));
            assert_eq!(d.line, f.line);
            assert_eq!(d.kind.as_deref(), Some("pack"));
            assert_eq!(d.source, DiagSource::Command(COMMAND.to_string()));
        }
        let membership = diags
            .iter()
            .find(|d| d.message.starts_with("declares the membership"))
            .unwrap();
        assert_eq!(membership.severity, Severity::Error);
        assert!(membership.line.is_some());
        assert!(diags.iter().any(|d| d.severity == Severity::Info));
    }

    #[test]
    fn a_private_pack_is_named_local_and_a_fork_keeps_its_name() {
        let presets = Path::new("/e/presets");
        assert_eq!(
            local_target(presets, Path::new("/home/packs/team-access.satz")).unwrap(),
            PathBuf::from("/e/presets/team-access.local.satz")
        );
        assert_eq!(
            local_target(presets, Path::new("/home/packs/team-access.local.satz")).unwrap(),
            PathBuf::from("/e/presets/team-access.local.satz")
        );
        assert!(matches!(
            local_name(Path::new("/p/essential-contacts.diff.satz")),
            Err(PlaceError::DiffFile(_))
        ));
        assert!(matches!(
            local_name(Path::new("/p/team-access.yaml")),
            Err(PlaceError::NotSatz(_))
        ));
        assert!(matches!(
            local_name(Path::new("/p/.local.satz")),
            Err(PlaceError::NotSatz(_))
        ));
    }

    #[test]
    fn upstream_the_pack_is_a_pristine_file_of_the_library() {
        assert_eq!(
            upstream_name(Path::new("/p/team-access.local.satz")).unwrap(),
            "presets/team-access.satz"
        );
        assert_eq!(
            upstream_name(Path::new("/p/team-access.satz")).unwrap(),
            "presets/team-access.satz"
        );
    }

    /// A checker that answers what it was built with and records that it ran.
    struct Answer(std::sync::Mutex<Option<Result<CompileSummary, CheckFailure>>>);

    impl Answer {
        fn pass() -> Answer {
            Answer(std::sync::Mutex::new(Some(Ok(CompileSummary {
                estate: "/e/yaml/C0example.satz".to_string(),
                addresses: Vec::new(),
                written: Vec::new(),
                findings: Vec::new(),
            }))))
        }
        fn refuse() -> Answer {
            Answer(std::sync::Mutex::new(Some(Err(CheckFailure::Refused(
                vec![Diagnostic::error("unknown param", DiagSource::Check)],
            )))))
        }
        fn ran(&self) -> bool {
            self.0.lock().unwrap().is_none()
        }
    }

    impl Checker for Answer {
        fn check<'a>(&'a self, _estate: &'a Path) -> crate::edit::CheckFuture<'a> {
            let answer = self.0.lock().unwrap().take().expect("checked once");
            Box::pin(async move { answer })
        }
    }

    fn reviewed(dir: &Path, text: &str) -> ReviewedPack {
        let path = dir.join("team-access.satz");
        std::fs::write(&path, text).unwrap();
        ReviewedPack {
            path,
            text: text.to_string(),
            sha256: sha256_hex(text.as_bytes()),
            against: false,
            review: serde_json::from_str(BROKEN).unwrap(),
        }
    }

    fn library() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let presets = tmp.path().join("presets");
        let packs = tmp.path().join("packs");
        std::fs::create_dir_all(&presets).unwrap();
        std::fs::create_dir_all(&packs).unwrap();
        (tmp, presets, packs)
    }

    #[tokio::test]
    async fn a_placed_pack_is_the_reviewed_bytes_under_its_local_name() {
        let (_tmp, presets, packs) = library();
        let pack = reviewed(&packs, "pack team_access version \"0.1\"\n");
        let checker = Answer::pass();
        let placed = place_private(&pack, &presets, Path::new("/e/x.satz"), &checker)
            .await
            .unwrap();
        let target = presets.join("team-access.local.satz");
        assert!(matches!(&placed, Placed::Written { path, .. } if *path == target));
        assert!(checker.ran());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), pack.text);
        assert!(!pack.is_in_library(&presets));
        // the placed file, reviewed where it stands, is the library's own
        let placed_review = ReviewedPack {
            path: plain(&target.canonicalize().unwrap()),
            ..pack.clone()
        };
        assert!(placed_review.is_in_library(&presets));

        // the same bytes again: nothing to write, and nothing checked
        let again = Answer::pass();
        assert_eq!(
            place_private(&pack, &presets, Path::new("/e/x.satz"), &again)
                .await
                .unwrap(),
            Placed::AlreadyThere(target)
        );
        assert!(!again.ran());
    }

    #[tokio::test]
    async fn a_fork_that_holds_other_text_is_refused_and_left_as_it_is() {
        let (_tmp, presets, packs) = library();
        let target = presets.join("team-access.local.satz");
        std::fs::write(&target, "// the estate's own fork\n").unwrap();
        let pack = reviewed(&packs, "pack team_access version \"0.1\"\n");
        let checker = Answer::pass();
        let e = place_private(&pack, &presets, Path::new("/e/x.satz"), &checker)
            .await
            .unwrap_err();
        assert!(matches!(&e, PlaceError::Exists(p) if *p == target), "{e}");
        assert!(!checker.ran());
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "// the estate's own fork\n"
        );
    }

    #[tokio::test]
    async fn a_pack_edited_since_its_review_is_refused() {
        let (_tmp, presets, packs) = library();
        let pack = reviewed(&packs, "pack team_access version \"0.1\"\n");
        std::fs::write(&pack.path, "pack team_access version \"0.2\"\n").unwrap();
        let e = place_private(&pack, &presets, Path::new("/e/x.satz"), &Answer::pass())
            .await
            .unwrap_err();
        assert!(matches!(e, PlaceError::ChangedSinceReview(_)), "{e}");
        assert!(!presets.join("team-access.local.satz").exists());
    }

    #[tokio::test]
    async fn a_placement_the_estate_refuses_is_removed_again() {
        let (_tmp, presets, packs) = library();
        let pack = reviewed(&packs, "pack team_access version \"0.1\"\n");
        let e = place_private(&pack, &presets, Path::new("/e/x.satz"), &Answer::refuse())
            .await
            .unwrap_err();
        assert!(matches!(&e, PlaceError::Rollback(d) if d.len() == 1), "{e}");
        assert!(!presets.join("team-access.local.satz").exists());
    }

    #[tokio::test]
    async fn an_estate_without_a_library_has_nowhere_to_put_a_pack() {
        let (tmp, _presets, packs) = library();
        let pack = reviewed(&packs, "pack team_access version \"0.1\"\n");
        let missing = tmp.path().join("no-presets");
        let e = place_private(&pack, &missing, Path::new("/e/x.satz"), &Answer::pass())
            .await
            .unwrap_err();
        assert!(matches!(e, PlaceError::NoLibrary(_)), "{e}");
    }
}
