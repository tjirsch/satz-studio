//! `McpSession` over the installed satz, rooted at the repository so the fixture's
//! paths into `vendor/satz` are inside the boundary.

use std::path::PathBuf;
use std::time::Duration;

use satz_studio_core::satz::reports::CompileSummary;
use satz_studio_core::satz::{Allow, McpSession, SatzBinary, SatzError};

const TIME_BOX: Duration = Duration::from_secs(60);

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

async fn open(allow: Allow) -> McpSession {
    let bin = SatzBinary::locate(None).await.unwrap();
    let root = repo_root();
    let config = root
        .join("tests")
        .join("fixtures")
        .join("smoke")
        .join("config.toml");
    let estate = root
        .join("vendor")
        .join("satz")
        .join("tests")
        .join("smoke")
        .join("yaml")
        .join("smoke.satz");
    tokio::time::timeout(
        TIME_BOX,
        McpSession::open(
            &bin,
            &root,
            allow,
            config.to_str().unwrap(),
            estate.to_str().unwrap(),
        ),
    )
    .await
    .unwrap()
    .unwrap()
}

#[tokio::test]
async fn the_session_opens_the_estate_and_holds_to_its_ceiling() {
    let session = open(Allow::Read).await;

    assert!(
        session.open_report().estate.ends_with("smoke.satz"),
        "{}",
        session.open_report().estate
    );
    assert!(
        session.open_report().config.ends_with("config.toml"),
        "{}",
        session.open_report().config
    );

    let outcome = tokio::time::timeout(
        TIME_BOX,
        session.call("satz_transpile_check", serde_json::Map::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!outcome.is_error, "{}", outcome.text);
    let summary: CompileSummary = outcome.typed("satz_transpile_check").unwrap();
    assert!(!summary.addresses.is_empty());
    assert!(
        summary.written.is_empty(),
        "a check writes nothing: {:?}",
        summary.written
    );

    let refused = tokio::time::timeout(
        TIME_BOX,
        session.call("satz_transpile", serde_json::Map::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(refused.is_error, "{}", refused.text);
    assert!(refused.text.contains("write"), "{}", refused.text);
    assert!(matches!(
        refused.typed::<CompileSummary>("satz_transpile"),
        Err(SatzError::Refused { .. })
    ));

    tokio::time::timeout(TIME_BOX, session.close())
        .await
        .unwrap()
        .unwrap();
}

/// An argument satz cannot read is a refusal the tool returns, and a name it does not
/// serve is a JSON-RPC `invalid_params` error, which the session names as such —
/// neither is a transport failure.
#[tokio::test]
async fn a_bad_argument_is_a_refusal_and_an_unknown_tool_is_invalid_params() {
    let session = open(Allow::Read).await;

    let mut mistyped = serde_json::Map::new();
    mistyped.insert("estate".to_string(), serde_json::json!(5));
    let refused = tokio::time::timeout(TIME_BOX, session.call("satz_transpile_check", mistyped))
        .await
        .unwrap()
        .unwrap();
    assert!(refused.is_error, "{refused:?}");
    assert!(refused.structured.is_none(), "{refused:?}");
    assert!(!refused.text.trim().is_empty(), "a refusal says why");

    let err = tokio::time::timeout(
        TIME_BOX,
        session.call("satz_imagined", serde_json::Map::new()),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(
        matches!(err, SatzError::InvalidParams { ref tool, ref message } if tool == "satz_imagined" && !message.is_empty()),
        "{err:?}"
    );

    tokio::time::timeout(TIME_BOX, session.close())
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn a_refused_open_is_an_error_not_a_session() {
    let bin = SatzBinary::locate(None).await.unwrap();
    let root = repo_root();
    let config = root
        .join("tests")
        .join("fixtures")
        .join("smoke")
        .join("config.toml");
    let estate = root
        .join("vendor")
        .join("satz")
        .join("tests")
        .join("smoke")
        .join("yaml")
        .join("nope.satz");
    let err = tokio::time::timeout(
        TIME_BOX,
        McpSession::open(
            &bin,
            &root,
            Allow::Read,
            config.to_str().unwrap(),
            estate.to_str().unwrap(),
        ),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(
        matches!(err, SatzError::Refused { ref tool, .. } if tool == "satz_open"),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_child_that_will_not_start_is_an_error_carrying_what_it_said() {
    let bin = SatzBinary::locate(None).await.unwrap();
    let root = repo_root()
        .join("tests")
        .join("fixtures")
        .join("does-not-exist");
    let err = tokio::time::timeout(
        TIME_BOX,
        McpSession::open(&bin, &root, Allow::Read, "config.toml", "smoke.satz"),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(
        matches!(err, SatzError::Mcp(ref text) if text.contains("does-not-exist")),
        "{err:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_child_that_exits_is_closed_on_the_next_call() {
    let session = open(Allow::Read).await;
    let pid = session.pid().expect("the child has a pid");
    let killed = std::process::Command::new("kill")
        .arg("-9")
        .arg(pid.to_string())
        .status()
        .unwrap();
    assert!(killed.success());
    // Whether the loop has already seen the pipe close or not: a send to the dead
    // child fails, and a request pending when the loop ends is dropped — both Closed.
    let err = tokio::time::timeout(
        TIME_BOX,
        session.call("satz_questions", serde_json::Map::new()),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(
        matches!(err, SatzError::Closed(_)),
        "expected Closed, got {err:?}"
    );
}
