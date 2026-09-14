//! The Claude Code CLI as the app finds it: the binary, its version, and what
//! `claude auth status` says. Offline — the fake CLI answers both.

use std::path::PathBuf;

use satz_studio_core::llm::claude_code::{ClaudeCodeCli, ClaudeCodeError};

#[path = "fixtures/claude_code/support.rs"]
mod support;

use support::{fake, fake_with, signed_in, signed_out, within};

#[tokio::test]
async fn an_override_that_is_there_is_run_for_its_version() {
    let tmp = tempfile::tempdir().unwrap();
    let cli = fake(tmp.path(), signed_out(), &[]).locate().await;
    assert_eq!(cli.version, "2.1.270");
}

#[tokio::test]
async fn an_override_that_is_not_there_names_itself_and_the_search_stops() {
    let missing = PathBuf::from("/nowhere/claude");
    let e = within(ClaudeCodeCli::locate(Some(&missing)))
        .await
        .unwrap_err();
    assert!(
        matches!(&e, ClaudeCodeError::NotFound { tried } if tried == &vec![missing.clone()]),
        "{e:?}"
    );
    assert!(e.to_string().contains("/nowhere/claude"), "{e}");
}

#[tokio::test]
async fn a_binary_that_prints_no_version_is_an_error_naming_what_it_printed() {
    let tmp = tempfile::tempdir().unwrap();
    let mut extra = serde_json::Map::new();
    extra.insert(
        "version".to_string(),
        serde_json::Value::String("Claude Code".to_string()),
    );
    let fake = fake_with(tmp.path(), signed_out(), &[], extra);
    let e = within(ClaudeCodeCli::locate(Some(&fake.path)))
        .await
        .unwrap_err();
    assert!(matches!(e, ClaudeCodeError::Protocol(_)), "{e:?}");
    assert!(e.to_string().contains("Claude Code"), "{e}");
}

#[tokio::test]
async fn a_signed_in_account_is_read_and_its_address_never_reaches_debug() {
    let tmp = tempfile::tempdir().unwrap();
    let cli = fake(tmp.path(), signed_in(), &[]).locate().await;
    let status = within(cli.auth_status()).await.unwrap();

    assert!(status.logged_in);
    assert!(status.is_subscription());
    assert_eq!(status.auth_method.as_deref(), Some("claude.ai"));
    assert_eq!(status.api_provider.as_deref(), Some("firstParty"));
    assert_eq!(status.email.as_deref(), Some("first.admin@example.com"));

    let printed = format!("{status:?}");
    assert!(
        !printed.contains('@'),
        "the address reached Debug: {printed}"
    );
    assert!(printed.contains("<redacted>"), "{printed}");
}

#[tokio::test]
async fn a_signed_out_cli_says_so_without_an_account() {
    let tmp = tempfile::tempdir().unwrap();
    let cli = fake(tmp.path(), signed_out(), &[]).locate().await;
    let status = within(cli.auth_status()).await.unwrap();

    assert!(!status.logged_in);
    assert!(!status.is_subscription());
    assert_eq!(status.email, None);
}

#[test]
fn the_sign_in_and_sign_out_lines_are_the_cli_with_its_path() {
    let cli = ClaudeCodeCli {
        path: PathBuf::from("/usr/local/bin/claude"),
        version: "2.1.270".to_string(),
    };
    assert_eq!(cli.login_command(), "\"/usr/local/bin/claude\" auth login");
    assert_eq!(
        cli.logout_command(),
        "\"/usr/local/bin/claude\" auth logout"
    );
}
