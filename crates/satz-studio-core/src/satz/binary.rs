//! The satz binary: where it is, which version it is, and the gate. An older satz than
//! the one this app was built against is refused at startup — the JSON shapes and the
//! tool names are that release's.

use std::path::{Path, PathBuf};

use super::SatzError;

/// The satz release the submodule `vendor/satz` is pinned to. The app refuses an older binary.
pub const MIN_SATZ: &str = "0.59.0";

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

    /// `satz` on `PATH`, else `~/.local/bin/satz`; [`SatzError::NotFound`] lists both.
    fn first_on_path_or_home() -> Result<PathBuf, SatzError> {
        let mut tried = Vec::new();
        match which::which("satz") {
            Ok(p) => return Ok(p),
            Err(_) => tried.push(PathBuf::from("satz (on PATH)")),
        }
        if let Some(home) = dirs::home_dir() {
            let local = home.join(".local").join("bin").join("satz");
            if local.is_file() {
                return Ok(local);
            }
            tried.push(local);
        }
        Err(SatzError::NotFound { tried })
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

    /// The gate: `found` must be at least [`MIN_SATZ`].
    pub fn check(path: PathBuf, found: semver::Version) -> Result<SatzBinary, SatzError> {
        let required = semver::Version::parse(MIN_SATZ).expect("MIN_SATZ is a version");
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

    /// The submodule `vendor/satz` is the one pin: `MIN_SATZ` must be the version it
    /// holds, or the app tests against one satz and demands another.
    #[test]
    fn min_satz_is_the_submodule_version() {
        let manifest = include_str!("../../../../vendor/satz/Cargo.toml");
        let version = manifest
            .lines()
            .find_map(|l| {
                l.strip_prefix("version = \"")
                    .and_then(|r| r.strip_suffix('"'))
            })
            .expect("vendor/satz/Cargo.toml has a version line");
        assert_eq!(
            version, MIN_SATZ,
            "vendor/satz is at {version} but MIN_SATZ says {MIN_SATZ}"
        );
    }

    #[test]
    fn the_pinned_version_passes() {
        let v = semver::Version::parse(MIN_SATZ).unwrap();
        assert!(SatzBinary::check(PathBuf::from("satz"), v).is_ok());
    }
}
