//! Keeping pace with satz, as the window says it: the notice for a satz newer than the
//! build, what the release looks found for satz-studio and for satz, the window's title,
//! and whether satz's installer is offered. Pure functions over the stores, so every
//! sentence the banner, the top bar, the title and Settings show is tested here.

use satz_studio_core::github::StudioUpdate;
use satz_studio_core::satz::self_update::SatzRelease;
use satz_studio_core::satz::{Ahead, SatzBinary};

use super::SatzStatus;

/// This build's own version.
pub const STUDIO_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The notice for a satz newer than the build: the binary and how far it is ahead, unless
/// the operator dismissed the notice for exactly that version. Nothing waits on it — a
/// newer satz is `Located` and runs either way.
pub fn satz_notice<'a>(
    status: &'a SatzStatus,
    dismissed: Option<&str>,
) -> Option<(&'a SatzBinary, Ahead)> {
    let binary = status.binary()?;
    let ahead = binary.ahead_of_build()?;
    if dismissed == Some(binary.version.to_string().as_str()) {
        return None;
    }
    Some((binary, ahead))
}

/// What a newer satz means for an estate, in the words of satz's own release rule (satz's
/// ADR 0010).
pub fn ahead_sentence(ahead: Ahead) -> &'static str {
    match ahead {
        Ahead::Patch => "It is a patch release: satz says nothing an estate needs changes.",
        Ahead::Minor => {
            "It is a minor release: satz says an estate may need edits, be refused, or plan differently."
        }
    }
}

/// The fact the notice states: which satz runs, which satz this build was tested against,
/// and what satz's rule says the difference means.
pub fn newer_satz_sentence(binary: &SatzBinary, ahead: Ahead) -> String {
    format!(
        "satz {} is installed; this satz-studio was built and tested against satz {}. {}",
        binary.version,
        SatzBinary::built_against(),
        ahead_sentence(ahead)
    )
}

/// What the last look for a satz-studio release found, as one sentence.
pub fn studio_look_sentence(outcome: &Result<StudioUpdate, String>) -> String {
    match outcome {
        Ok(StudioUpdate::Available { version, .. }) => format!(
            "satz-studio {version} is released; this is {STUDIO_VERSION}. Its release page has the bundles — satz-studio does not update itself."
        ),
        Ok(StudioUpdate::Latest { latest }) if latest.to_string() == STUDIO_VERSION => {
            format!("satz-studio {STUDIO_VERSION} is the latest release.")
        }
        Ok(StudioUpdate::Latest { latest }) => {
            format!("satz-studio {STUDIO_VERSION} is newer than the latest release, {latest}.")
        }
        Err(why) => format!("The look for a satz-studio release failed: {why}"),
    }
}

/// What the last `satz self-update --check-only` found, as one sentence; `current` is the
/// satz that was asked.
pub fn satz_release_sentence(
    found: &Result<SatzRelease, String>,
    current: &semver::Version,
) -> String {
    match found {
        Ok(SatzRelease::Available { latest, .. }) => format!(
            "satz {latest} is released; this is {current}. Update satz installs it with satz's own updater."
        ),
        Ok(SatzRelease::Latest { latest }) if latest == current => {
            format!("satz {current} is the latest release.")
        }
        Ok(SatzRelease::Latest { latest }) => {
            format!("satz {current} is newer than the latest release, {latest}.")
        }
        Err(why) => format!("The look for a satz release failed: {why}"),
    }
}

/// The newer satz release a check found, while it is still newer than `running`, the satz
/// in use — what the top bar and the title offer to install.
pub fn satz_available<'a>(
    found: Option<&'a Result<SatzRelease, String>>,
    running: Option<&semver::Version>,
) -> Option<&'a semver::Version> {
    match found? {
        Ok(SatzRelease::Available { latest, .. }) => match running {
            Some(running) if latest <= running => None,
            _ => Some(latest),
        },
        _ => None,
    }
}

/// The newer satz-studio release a look found.
pub fn studio_available(
    outcome: Option<&Result<StudioUpdate, String>>,
) -> Option<(&semver::Version, &str)> {
    match outcome? {
        Ok(StudioUpdate::Available { version, page }) => Some((version, page.as_str())),
        _ => None,
    }
}

/// The window's title: the app and its version, then what the looks found available. A
/// look that failed or found nothing adds nothing — the title is not where a failure is
/// read; Settings says why.
pub fn window_title(studio: Option<&semver::Version>, satz: Option<&semver::Version>) -> String {
    let mut title = format!("satz-studio {STUDIO_VERSION}");
    if let Some(version) = studio {
        title.push_str(&format!(" — update available: satz-studio {version}"));
    }
    if let Some(version) = satz {
        title.push_str(&format!(" — update available: satz {version}"));
    }
    title
}

/// Whether satz's installer is offered while no satz is found, and the sentence that says
/// what can be done instead when it is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallOffer {
    Offered,
    /// Settings name a satz binary: the installer writes `~/.local/bin/satz`, which the
    /// search does not look at while a path is set
    PathSet,
    /// satz publishes no Windows build
    NoWindowsBuild,
}

impl InstallOffer {
    pub fn offered(self) -> bool {
        self == InstallOffer::Offered
    }

    pub fn sentence(self) -> &'static str {
        match self {
            InstallOffer::Offered => {
                "Install satz here — satz's own installer, checked against the SHA-256 its release publishes, writes ~/.local/bin/satz and leaves your shell profile alone — or set its path in Settings."
            }
            InstallOffer::PathSet => {
                "Correct the satz path in Settings, or clear it to have satz installed into ~/.local/bin."
            }
            InstallOffer::NoWindowsBuild => {
                "satz publishes no Windows build, so satz-studio cannot install it here: build satz from source and set its path in Settings."
            }
        }
    }
}

/// The offer on this system, with or without a satz path in Settings.
pub fn install_offer(path_set: bool) -> InstallOffer {
    install_offer_on(cfg!(windows), path_set)
}

fn install_offer_on(windows: bool, path_set: bool) -> InstallOffer {
    if windows {
        InstallOffer::NoWindowsBuild
    } else if path_set {
        InstallOffer::PathSet
    } else {
        InstallOffer::Offered
    }
}

/// The fake satz binaries of the core crate's tests: a script named `satz` that prints what
/// it is given, proven runnable before it is returned.
#[cfg(all(test, unix))]
#[path = "../../../satz-studio-core/tests/fixtures/satz/support.rs"]
mod fake_satz;

#[cfg(test)]
mod tests {
    use satz_studio_core::settings::Settings;

    use super::*;

    /// The satz this build is tested against, moved by `minor` or else by `patch`, so the
    /// tests hold across every pin move.
    fn beside_the_build(minor: u64, patch: u64) -> semver::Version {
        let built = SatzBinary::built_against();
        if minor > 0 {
            semver::Version::new(built.major, built.minor + minor, 0)
        } else {
            semver::Version::new(built.major, built.minor, built.patch + patch)
        }
    }

    /// The status the app coroutine reaches for a fake satz at `version`, as `locate` in
    /// `app_actions.rs` reaches it.
    #[cfg(unix)]
    async fn status_of(version: &semver::Version) -> SatzStatus {
        let tmp = tempfile::tempdir().unwrap();
        let path = fake_satz::fake(tmp.path(), &format!("echo 'satz {version}'"));
        match SatzBinary::locate(Some(&path)).await {
            Ok(binary) => SatzStatus::Located(binary),
            Err(e) => SatzStatus::Unusable(e.to_string()),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_pinned_satz_opens_estates_with_no_notice() {
        let status = status_of(&SatzBinary::built_against()).await;
        assert!(status.binary().is_some(), "{status:?}");
        assert_eq!(satz_notice(&status, None), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_older_satz_is_the_one_refusal() {
        let tmp = tempfile::tempdir().unwrap();
        let path = fake_satz::fake(tmp.path(), "echo 'satz 0.51.1'");
        let e = SatzBinary::locate(Some(&path)).await.unwrap_err();
        assert!(
            matches!(e, satz_studio_core::satz::SatzError::TooOld { .. }),
            "{e:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_newer_patch_opens_estates_and_is_told_as_a_patch() {
        let patch = beside_the_build(0, 1);
        let status = status_of(&patch).await;
        assert!(status.binary().is_some(), "a newer satz opens estates");
        let (binary, ahead) = satz_notice(&status, None).expect("a notice");
        assert_eq!(binary.version, patch);
        assert_eq!(ahead, Ahead::Patch);
        let said = newer_satz_sentence(binary, ahead);
        assert!(
            said.contains(&format!("satz {patch} is installed")),
            "{said}"
        );
        assert!(
            said.contains(&format!(
                "tested against satz {}",
                SatzBinary::built_against()
            )),
            "{said}"
        );
        assert!(said.contains("nothing an estate needs changes"), "{said}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_newer_minor_opens_estates_and_is_told_as_a_minor() {
        let status = status_of(&beside_the_build(1, 0)).await;
        assert!(status.binary().is_some());
        let (binary, ahead) = satz_notice(&status, None).expect("a notice");
        assert_eq!(ahead, Ahead::Minor);
        assert!(
            newer_satz_sentence(binary, ahead)
                .contains("may need edits, be refused, or plan differently")
        );
    }

    /// A dismissed notice stays dismissed for exactly that satz release; the next newer
    /// release is told again. The dismissal changes nothing else: the satz runs either way.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_dismissed_notice_stays_gone_for_that_version_only() {
        let patch = beside_the_build(0, 1);
        let dismissed = patch.to_string();
        let status = status_of(&patch).await;
        assert_eq!(satz_notice(&status, Some(&dismissed)), None);
        assert!(status.binary().is_some());

        let later = status_of(&beside_the_build(0, 2)).await;
        assert!(satz_notice(&later, Some(&dismissed)).is_some());
    }

    #[test]
    fn the_dismissed_version_round_trips_through_the_settings_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let settings = Settings {
            dismissed_satz: Some(beside_the_build(1, 0).to_string()),
            ..Settings::default()
        };
        settings.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path).unwrap(), settings);
    }

    #[test]
    fn the_studio_look_says_which_of_three_things_it_found() {
        let running = semver::Version::parse(STUDIO_VERSION).unwrap();
        let newer = semver::Version::new(running.major, running.minor + 1, 0);
        let available = Ok(StudioUpdate::Available {
            version: newer.clone(),
            page: "https://github.com/tjirsch/satz-studio/releases".to_string(),
        });
        let said = studio_look_sentence(&available);
        assert!(
            said.contains(&format!("satz-studio {newer} is released")),
            "{said}"
        );
        assert!(said.contains("does not update itself"), "{said}");
        assert_eq!(
            studio_available(Some(&available)),
            Some((&newer, "https://github.com/tjirsch/satz-studio/releases"))
        );

        let latest = Ok(StudioUpdate::Latest {
            latest: running.clone(),
        });
        assert!(studio_look_sentence(&latest).contains("is the latest release"));
        assert_eq!(studio_available(Some(&latest)), None);

        let failed = Err(
            "GitHub refused the request: the unauthenticated API allows 60 requests an hour"
                .to_string(),
        );
        let said = studio_look_sentence(&failed);
        assert!(
            said.starts_with("The look for a satz-studio release failed:"),
            "{said}"
        );
        assert!(said.contains("60 requests an hour"), "{said}");
        assert_eq!(studio_available(Some(&failed)), None);
        assert_eq!(studio_available(None), None);
    }

    #[test]
    fn the_satz_check_offers_a_release_only_while_it_is_newer_than_the_satz_that_runs() {
        let built = SatzBinary::built_against();
        let newer = beside_the_build(1, 0);
        let found = Ok(SatzRelease::Available {
            latest: newer.clone(),
            page: None,
        });
        assert_eq!(satz_available(Some(&found), Some(&built)), Some(&newer));
        let said = satz_release_sentence(&found, &built);
        assert!(
            said.contains(&format!("satz {newer} is released")),
            "{said}"
        );

        // once satz has updated to it, the same finding offers nothing
        assert_eq!(satz_available(Some(&found), Some(&newer)), None);

        let latest = Ok(SatzRelease::Latest {
            latest: built.clone(),
        });
        assert_eq!(satz_available(Some(&latest), Some(&built)), None);
        assert!(satz_release_sentence(&latest, &built).contains("is the latest release"));

        let failed =
            Err("GitHub API rate limit reached (60 requests/hour, unauthenticated)".to_string());
        assert_eq!(satz_available(Some(&failed), Some(&built)), None);
        assert!(
            satz_release_sentence(&failed, &built)
                .starts_with("The look for a satz release failed:")
        );
    }

    #[test]
    fn the_title_carries_the_version_and_what_is_available() {
        assert_eq!(
            window_title(None, None),
            format!("satz-studio {STUDIO_VERSION}")
        );
        let studio = semver::Version::new(9, 0, 0);
        let satz = beside_the_build(1, 0);
        assert_eq!(
            window_title(Some(&studio), Some(&satz)),
            format!(
                "satz-studio {STUDIO_VERSION} — update available: satz-studio 9.0.0 — update available: satz {satz}"
            )
        );
    }

    #[test]
    fn windows_is_never_offered_the_installer_and_says_why() {
        for path_set in [false, true] {
            let offer = install_offer_on(true, path_set);
            assert_eq!(offer, InstallOffer::NoWindowsBuild);
            assert!(!offer.offered());
            assert!(offer.sentence().contains("publishes no Windows build"));
        }
        assert!(install_offer_on(false, false).offered());
        assert!(
            install_offer_on(false, false)
                .sentence()
                .contains("leaves your shell profile alone")
        );
        assert_eq!(install_offer_on(false, true), InstallOffer::PathSet);
        assert!(!install_offer_on(false, true).offered());
    }
}
