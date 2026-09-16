//! What `satz self-update --check-only` found, and whether the operator lets satz look for
//! releases on its own.
//!
//! satz owns its updater: it asks GitHub for its latest release, and the app runs the
//! command rather than reading GitHub for satz a second time. The command prints for a
//! person, not for a program, so the reading here is narrow: the `Latest version: X` line
//! is the whole answer, compared with the version of the binary that printed it, and the
//! `Release:` line is carried when satz prints one. An output without the line is an error
//! naming what satz printed, never a guess.
//!
//! The operator's own satz setting, `self_update_frequency` in `~/.config/satz/satz.toml`,
//! says whether satz may look for releases unprompted. `never` is read as "not on my
//! behalf either": the app then looks only when asked.

use std::path::{Path, PathBuf};

/// What the check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SatzRelease {
    /// a release newer than the satz that was asked; `page` is the release page satz named
    Available {
        latest: semver::Version,
        page: Option<String>,
    },
    /// the satz that was asked is the latest release, or newer
    Latest { latest: semver::Version },
}

/// Read the stdout of `satz self-update --check-only`, run on the satz at `current`.
pub fn read_check(stdout: &str, current: &semver::Version) -> Result<SatzRelease, String> {
    let latest = stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("Latest version:"))
        .map(str::trim)
        .ok_or_else(|| {
            format!(
                "satz self-update --check-only printed no `Latest version:` line: {:?}",
                stdout.chars().take(300).collect::<String>()
            )
        })?;
    let latest = semver::Version::parse(latest.trim_start_matches('v')).map_err(|e| {
        format!("satz named its latest release `{latest}`, which is not a version: {e}")
    })?;
    if latest > *current {
        let page = stdout
            .lines()
            .find_map(|l| l.trim().strip_prefix("Release:"))
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());
        Ok(SatzRelease::Available { latest, page })
    } else {
        Ok(SatzRelease::Latest { latest })
    }
}

/// `~/.config/satz/satz.toml` under `home`, where satz keeps `self_update_frequency`.
pub fn satz_config_path(home: &Path) -> PathBuf {
    home.join(".config").join("satz").join("satz.toml")
}

/// Whether the operator lets satz look for releases on its own, read as satz reads it: a
/// missing file or a missing key is satz's default, `always`; `never` is no; `always`
/// and `daily` are yes. A file that exists and does not parse is an error naming it — satz
/// refuses to run with it too.
pub fn unprompted_checks_allowed(home: Option<&Path>) -> Result<bool, String> {
    let Some(home) = home else {
        return Ok(true);
    };
    let path = satz_config_path(home);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let table: toml::Table =
        toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    match table.get("self_update_frequency") {
        None => Ok(true),
        Some(toml::Value::String(frequency)) => Ok(frequency != "never"),
        Some(other) => Err(format!(
            "{}: self_update_frequency is {other}, not a string",
            path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    /// The lines satz's `run_self_update` prints when a newer release exists.
    const AVAILABLE: &str = "Current version: 0.59.7\nLatest version: 0.60.0\n\n⚠️  A new version is available!\n   Current: 0.59.7\n   Latest:  0.60.0\n   Release: https://github.com/tjirsch/satz/releases/tag/v0.60.0\n\nRun `satz self-update` to install.\n";

    /// ... and when the satz asked is the latest.
    const LATEST: &str =
        "Current version: 0.59.7\nLatest version: 0.59.7\n✅ You are running the latest version!\n";

    #[test]
    fn a_newer_release_is_read_with_its_page() {
        assert_eq!(
            read_check(AVAILABLE, &v("0.59.7")).unwrap(),
            SatzRelease::Available {
                latest: v("0.60.0"),
                page: Some("https://github.com/tjirsch/satz/releases/tag/v0.60.0".to_string()),
            }
        );
    }

    #[test]
    fn the_latest_release_or_an_older_one_is_latest() {
        assert_eq!(
            read_check(LATEST, &v("0.59.7")).unwrap(),
            SatzRelease::Latest {
                latest: v("0.59.7")
            }
        );
        // a satz built from a newer source than the latest release
        assert_eq!(
            read_check(LATEST, &v("0.59.8")).unwrap(),
            SatzRelease::Latest {
                latest: v("0.59.7")
            }
        );
    }

    #[test]
    fn an_output_without_the_line_is_an_error_naming_what_satz_printed() {
        let e = read_check("Current version: 0.59.7\n", &v("0.59.7")).unwrap_err();
        assert!(e.contains("Latest version:"), "{e}");
        assert!(e.contains("Current version: 0.59.7"), "{e}");
        let e = read_check("Latest version: soon\n", &v("0.59.7")).unwrap_err();
        assert!(e.contains("`soon`"), "{e}");
    }

    #[test]
    fn never_in_the_operators_satz_config_is_the_one_no() {
        let home = tempfile::tempdir().unwrap();
        // no file: satz's default, always
        assert_eq!(unprompted_checks_allowed(Some(home.path())), Ok(true));
        assert_eq!(unprompted_checks_allowed(None), Ok(true));

        let path = satz_config_path(home.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        for (text, allowed) in [
            ("self_update_frequency = \"never\"\n", false),
            (
                "self_update_frequency = \"daily\"\nlast_update_check = \"1\"\n",
                true,
            ),
            ("self_update_frequency = \"always\"\n", true),
            ("last_update_check = \"1\"\n", true),
        ] {
            std::fs::write(&path, text).unwrap();
            assert_eq!(
                unprompted_checks_allowed(Some(home.path())),
                Ok(allowed),
                "{text}"
            );
        }

        std::fs::write(&path, "self_update_frequency = [").unwrap();
        let e = unprompted_checks_allowed(Some(home.path())).unwrap_err();
        assert!(e.contains("satz.toml"), "{e}");
    }
}
