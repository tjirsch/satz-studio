//! `SatzBinary::locate`: the override, the search, the version gate — against fake
//! binaries in a temporary directory and the installed satz.

use std::time::Duration;

#[cfg(unix)]
use satz_studio_core::satz::Ahead;
use satz_studio_core::satz::{MIN_SATZ, SatzBinary, SatzError};

// the fake binaries are a unix fixture, and so are the tests that use them
#[cfg(unix)]
#[path = "fixtures/satz/support.rs"]
mod support;

#[cfg(unix)]
use support::fake;

const TIME_BOX: Duration = Duration::from_secs(60);

#[cfg(unix)]
#[tokio::test]
async fn an_old_fake_is_refused_as_too_old() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fake(tmp.path(), "echo 'satz 0.51.1'");
    let err = tokio::time::timeout(TIME_BOX, SatzBinary::locate(Some(&path)))
        .await
        .unwrap()
        .unwrap_err();
    let said = err.to_string();
    match err {
        SatzError::TooOld {
            path: refused,
            found,
            required,
        } => {
            assert_eq!(found, semver::Version::new(0, 51, 1));
            assert_eq!(required, semver::Version::parse(MIN_SATZ).unwrap());
            // The refusal names the binary it refused. The app offers to run
            // `self-update` on exactly that one, which is the only way out of the refusal
            // without leaving the window, so losing the path here would cost the offer.
            assert_eq!(refused, path);
            assert!(
                said.contains(&refused.display().to_string()),
                "the message names the binary: {said}"
            );
        }
        other => panic!("expected TooOld, got {other:?}"),
    }
}

/// A fake satz at `version`, located the way the app locates the binary Settings name.
#[cfg(unix)]
async fn located_at(version: &semver::Version) -> Result<SatzBinary, SatzError> {
    let tmp = tempfile::tempdir().unwrap();
    let path = fake(tmp.path(), &format!("echo 'satz {version}'"));
    tokio::time::timeout(TIME_BOX, SatzBinary::locate(Some(&path)))
        .await
        .unwrap()
}

/// The driver is permissive upward: a satz newer than the build's is located, not refused,
/// and says by which kind of release it is newer; the app runs it and tells the operator.
/// CI installs the NEWEST satz release on purpose, so every test that locates the real
/// binary sees a newer satz on the day after a satz release — which is why nothing refuses
/// it. The versions are derived from the vendored satz so the test holds across pin moves.
#[cfg(unix)]
#[tokio::test]
async fn a_fake_at_the_pin_or_newer_is_located_and_says_how_far_ahead_it_is() {
    let built = SatzBinary::built_against();

    let pinned = located_at(&built).await.unwrap();
    assert_eq!(pinned.version, built);
    assert_eq!(pinned.ahead_of_build(), None);

    let patch = semver::Version::new(built.major, built.minor, built.patch + 1);
    let newer_patch = located_at(&patch).await.unwrap();
    assert_eq!(newer_patch.version, patch);
    assert_eq!(newer_patch.ahead_of_build(), Some(Ahead::Patch));

    let minor = semver::Version::new(built.major, built.minor + 1, 0);
    let newer_minor = located_at(&minor).await.unwrap();
    assert_eq!(newer_minor.version, minor);
    assert_eq!(newer_minor.ahead_of_build(), Some(Ahead::Minor));
}

#[cfg(unix)]
#[tokio::test]
async fn a_fake_that_prints_no_version_is_unparsable() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fake(tmp.path(), "echo 'hello there'");
    let err = tokio::time::timeout(TIME_BOX, SatzBinary::locate(Some(&path)))
        .await
        .unwrap()
        .unwrap_err();
    assert!(
        matches!(err, SatzError::VersionUnparsable(ref text) if text.contains("hello there")),
        "{err:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_fake_that_exits_non_zero_is_an_error_naming_the_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fake(tmp.path(), "echo 'broken install' >&2; exit 3");
    let err = tokio::time::timeout(TIME_BOX, SatzBinary::locate(Some(&path)))
        .await
        .unwrap()
        .unwrap_err();
    assert!(
        matches!(err, SatzError::Exit { ref stderr, .. } if stderr.contains("broken install")),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_missing_override_is_not_found_and_named() {
    let tmp = tempfile::tempdir().unwrap();
    let nope = tmp.path().join("nope").join("satz");
    let err = tokio::time::timeout(TIME_BOX, SatzBinary::locate(Some(&nope)))
        .await
        .unwrap()
        .unwrap_err();
    assert!(
        matches!(err, SatzError::NotFound { ref tried } if tried == &vec![nope.clone()]),
        "{err:?}"
    );
    assert!(err.to_string().contains(&nope.display().to_string()));
}

#[tokio::test]
async fn the_installed_satz_is_found_at_the_pinned_version_or_newer() {
    let bin = tokio::time::timeout(TIME_BOX, SatzBinary::locate(None))
        .await
        .unwrap()
        .unwrap();
    assert!(bin.path.is_file(), "{}", bin.path.display());
    assert!(
        bin.version >= semver::Version::parse(MIN_SATZ).unwrap(),
        "found {} but MIN_SATZ is {MIN_SATZ}",
        bin.version
    );
}
