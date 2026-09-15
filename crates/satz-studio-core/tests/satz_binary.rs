//! `SatzBinary::locate`: the override, the search, the version gate — against fake
//! binaries in a temporary directory and the installed satz.

use std::time::Duration;

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
    match err {
        SatzError::TooOld { found, required } => {
            assert_eq!(found, semver::Version::new(0, 51, 1));
            assert_eq!(required, semver::Version::parse(MIN_SATZ).unwrap());
        }
        other => panic!("expected TooOld, got {other:?}"),
    }
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
