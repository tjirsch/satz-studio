//! The satz binary: where it is, which version it is, and the gate. A satz older than
//! [`MIN_SATZ`] is refused — the JSON shapes and the tool names the app reads are not that
//! release's. A satz NEWER than the one this build is built and tested against
//! ([`SatzBinary::built_against`]) is located and reported as newer ([`Ahead`]), never
//! refused: the app copes with a newer satz and tells the operator, and CI drives the newest
//! satz release on purpose.

use std::path::{Path, PathBuf};

use super::SatzError;

/// The oldest satz this build works with; an older binary is refused. It may sit below the
/// submodule's satz ([`SatzBinary::built_against`]), and rises only when a satz release
/// breaks the app or the app starts using something a later satz introduced — a routine pin
/// bump leaves it alone (ADR 0014).
pub const MIN_SATZ: &str = "0.84.0";

/// The manifest of the submodule `vendor/satz`, whose version is the satz this build is
/// built and tested against.
const VENDORED_MANIFEST: &str = include_str!("../../../../vendor/satz/Cargo.toml");

/// How far a located satz is past the satz this build is built and tested against
/// ([`SatzBinary::built_against`]), read by satz's own release rule (satz's
/// ADR 0010): a MINOR release is one after which the same estate or input needs an edit,
/// is refused, or plans differently; everything else is a PATCH. A major number that moved
/// is read as a minor, the kind that brings work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ahead {
    /// the same major and minor, a later patch: satz says nothing an estate needs changes
    Patch,
    /// a later minor or major: satz says an estate may need edits, be refused, or plan
    /// differently
    Minor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SatzBinary {
    pub path: PathBuf,
    pub version: semver::Version,
}

impl SatzBinary {
    /// Settings override, then `PATH`, then `~/.local/bin/satz`; the first that exists is
    /// run with `--version` and held to [`MIN_SATZ`]. An override that does not exist is
    /// [`SatzError::NotFound`] naming it; a candidate that exists but does not run, or
    /// prints no version, is an error naming it — the search never continues past it.
    pub async fn locate(override_path: Option<&Path>) -> Result<SatzBinary, SatzError> {
        let path = match override_path {
            Some(p) if p.is_file() => p.to_path_buf(),
            Some(p) => {
                return Err(SatzError::NotFound {
                    tried: vec![p.to_path_buf()],
                });
            }
            None => Self::first_on_path_or_home()?,
        };
        let output = tokio::process::Command::new(&path)
            .arg("--version")
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|e| SatzError::Io {
                context: format!("running `{} --version`", path.display()),
                source: e,
            })?;
        if !output.status.success() {
            return Err(SatzError::Exit {
                command: "--version".to_string(),
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        // The version line is on stdout, the banner on stderr; both are read so a
        // binary that prints only the banner still names its version.
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push('\n');
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        let version = Self::parse_version(&text)?;
        Self::check(path, version)
    }

    /// `satz` on `PATH`, else `~/.local/bin/satz` (`satz.exe` on Windows);
    /// [`SatzError::NotFound`] lists both. The home lookup is what finds a satz installed
    /// while the app runs: the app keeps the `PATH` it was started with.
    fn first_on_path_or_home() -> Result<PathBuf, SatzError> {
        let mut tried = Vec::new();
        match which::which("satz") {
            Ok(p) => return Ok(p),
            Err(_) => tried.push(PathBuf::from("satz (on PATH)")),
        }
        if let Some(dir) = Self::home_bin_dir() {
            let local = Self::in_dir(&dir, std::env::consts::EXE_SUFFIX);
            if local.is_file() {
                return Ok(local);
            }
            tried.push(local);
        }
        Err(SatzError::NotFound { tried })
    }

    /// `~/.local/bin`, where satz's installer puts satz and where the search looks after
    /// `PATH`; `None` without a home directory.
    pub fn home_bin_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|home| home.join(".local").join("bin"))
    }

    /// The satz binary in `dir`, with the platform's executable suffix: `""`, or `".exe"`
    /// on Windows (`std::env::consts::EXE_SUFFIX`).
    pub fn in_dir(dir: &Path, exe_suffix: &str) -> PathBuf {
        dir.join(format!("satz{exe_suffix}"))
    }

    /// The version in `satz --version` output (`satz 0.56.1`, possibly after the banner line).
    pub fn parse_version(output: &str) -> Result<semver::Version, SatzError> {
        for line in output.lines() {
            if let Some(rest) = line.trim().strip_prefix("satz ") {
                let word = rest
                    .trim_start_matches('v')
                    .split_whitespace()
                    .next()
                    .unwrap_or("");
                if let Ok(v) = semver::Version::parse(word) {
                    return Ok(v);
                }
            }
        }
        Err(SatzError::VersionUnparsable(output.to_string()))
    }

    /// [`MIN_SATZ`] as a version.
    pub fn minimum() -> semver::Version {
        semver::Version::parse(MIN_SATZ).expect("MIN_SATZ is a version")
    }

    /// The satz this build is built and tested against: the version of the submodule
    /// `vendor/satz`, read from its manifest as the crate compiled, so it cannot name
    /// another release than the one the tests ran over.
    pub fn built_against() -> semver::Version {
        let version = VENDORED_MANIFEST
            .lines()
            .find_map(|l| {
                l.strip_prefix("version = \"")
                    .and_then(|r| r.strip_suffix('"'))
            })
            .expect("vendor/satz/Cargo.toml has a version line");
        semver::Version::parse(version).expect("vendor/satz/Cargo.toml's version is a version")
    }

    /// Whether this satz is past the one this build is built and tested against, and by
    /// which kind of release. `None` is that release itself, or an older one. Only the three numbers are read:
    /// a build-metadata suffix on the pinned release is that release.
    pub fn ahead_of_build(&self) -> Option<Ahead> {
        let built = Self::built_against();
        let found = &self.version;
        let numbers = |v: &semver::Version| (v.major, v.minor, v.patch);
        if numbers(found) <= numbers(&built) {
            None
        } else if (found.major, found.minor) == (built.major, built.minor) {
            Some(Ahead::Patch)
        } else {
            Some(Ahead::Minor)
        }
    }

    /// The gate: `found` must be at least [`MIN_SATZ`]. A newer `found` passes, and
    /// [`Self::ahead_of_build`] says by how much.
    pub fn check(path: PathBuf, found: semver::Version) -> Result<SatzBinary, SatzError> {
        let required = Self::minimum();
        if found < required {
            return Err(SatzError::TooOld {
                path,
                found,
                required,
            });
        }
        Ok(SatzBinary {
            path,
            version: found,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_home_binary_carries_the_platform_s_executable_suffix() {
        let dir = PathBuf::from("home").join(".local").join("bin");
        assert_eq!(SatzBinary::in_dir(&dir, ""), dir.join("satz"));
        assert_eq!(SatzBinary::in_dir(&dir, ".exe"), dir.join("satz.exe"));
        let here = SatzBinary::in_dir(&dir, std::env::consts::EXE_SUFFIX);
        assert_eq!(
            here.file_name().unwrap(),
            if cfg!(windows) { "satz.exe" } else { "satz" }
        );
    }

    #[test]
    fn the_version_line_is_read_past_the_banner() {
        let out = "satz v0.56.1 (built 2026-09-13 13:56:42)\nsatz 0.56.1\n";
        assert_eq!(
            SatzBinary::parse_version(out).unwrap(),
            semver::Version::new(0, 56, 1)
        );
    }

    #[test]
    fn an_older_binary_is_refused_by_version() {
        let e = SatzBinary::check(PathBuf::from("/opt/satz"), semver::Version::new(0, 51, 1))
            .unwrap_err();
        let SatzError::TooOld { ref path, .. } = e else {
            panic!("expected TooOld, got {e:?}");
        };
        assert_eq!(path, &PathBuf::from("/opt/satz"));
        // The message names the binary and both versions. It does NOT tell the operator to
        // run `satz self-update`: the app offers that, on this path, rather than printing a
        // terminal instruction into a window.
        let said = e.to_string();
        assert!(said.contains("/opt/satz"), "{said}");
        assert!(said.contains("0.51.1"), "{said}");
        assert!(said.contains(MIN_SATZ), "{said}");
    }

    /// `MIN_SATZ` is the oldest satz this build works with and may sit below the submodule
    /// `vendor/satz`, which the tests run over; above it, the app would demand a satz it was
    /// never tested with.
    #[test]
    fn min_satz_is_not_newer_than_the_submodule() {
        let vendored = SatzBinary::built_against();
        assert!(
            SatzBinary::minimum() <= vendored,
            "MIN_SATZ says {MIN_SATZ} but vendor/satz is at {vendored}"
        );
    }

    #[test]
    fn the_pinned_version_passes() {
        let v = semver::Version::parse(MIN_SATZ).unwrap();
        assert!(SatzBinary::check(PathBuf::from("satz"), v).is_ok());
    }

    fn located(version: semver::Version) -> SatzBinary {
        SatzBinary::check(PathBuf::from("/opt/satz"), version).expect("not older than MIN_SATZ")
    }

    /// The versions are derived from `MIN_SATZ`, so the test holds across every pin move.
    #[test]
    fn a_newer_satz_passes_the_gate_and_says_by_which_kind_of_release() {
        let built = SatzBinary::built_against();
        assert_eq!(located(built.clone()).ahead_of_build(), None);

        let patch = semver::Version::new(built.major, built.minor, built.patch + 1);
        assert_eq!(located(patch).ahead_of_build(), Some(Ahead::Patch));

        let minor = semver::Version::new(built.major, built.minor + 1, 0);
        assert_eq!(located(minor).ahead_of_build(), Some(Ahead::Minor));

        // a major number that moved brings at least the work a minor brings
        let major = semver::Version::new(built.major + 1, 0, 0);
        assert_eq!(located(major).ahead_of_build(), Some(Ahead::Minor));
    }

    #[test]
    fn build_metadata_on_the_pinned_release_is_that_release() {
        let mut v = SatzBinary::built_against();
        v.build = semver::BuildMetadata::new("dev").unwrap();
        assert_eq!(located(v).ahead_of_build(), None);
    }
}
