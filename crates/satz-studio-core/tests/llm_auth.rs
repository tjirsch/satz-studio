//! `Credential::resolve`: the environment branches, serialised through one lock since
//! the environment is process-wide; the keychain branch only with
//! `SATZ_STUDIO_KEYCHAIN_TEST=1`, so a plain `cargo test` never touches a keychain.

use satz_studio_core::llm::{ClaudeError, Credential, CredentialSource};

static ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Sets variables for the test and restores every one of them on drop.
struct Env {
    saved: Vec<(&'static str, Option<String>)>,
}

impl Env {
    fn set(vars: &[(&'static str, Option<&str>)]) -> Self {
        let saved = vars
            .iter()
            .map(|(name, _)| (*name, std::env::var(name).ok()))
            .collect();
        for (name, value) in vars {
            // SAFETY: every test that touches the environment holds `ENV`, and no other
            // thread of this test binary reads these variables meanwhile.
            unsafe {
                match value {
                    Some(v) => std::env::set_var(name, v),
                    None => std::env::remove_var(name),
                }
            }
        }
        Self { saved }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            // SAFETY: as in `set`.
            unsafe {
                match value {
                    Some(v) => std::env::set_var(name, v),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}

#[tokio::test]
async fn the_api_key_variable_wins() {
    let _lock = ENV.lock().await;
    let _env = Env::set(&[
        ("ANTHROPIC_API_KEY", Some("sk-example")),
        ("ANTHROPIC_AUTH_TOKEN", Some("tok-example")),
    ]);
    let (credential, source) = Credential::resolve().await.expect("resolves");
    assert_eq!(credential, Credential::ApiKey("sk-example".to_string()));
    assert_eq!(source, CredentialSource::ApiKeyEnv);
}

#[tokio::test]
async fn the_auth_token_variable_is_second_and_a_blank_variable_is_unset() {
    let _lock = ENV.lock().await;
    let _env = Env::set(&[
        ("ANTHROPIC_API_KEY", Some("   ")),
        ("ANTHROPIC_AUTH_TOKEN", Some(" tok-example\n")),
    ]);
    let (credential, source) = Credential::resolve().await.expect("resolves");
    assert_eq!(credential, Credential::Bearer("tok-example".to_string()));
    assert_eq!(source, CredentialSource::AuthTokenEnv);
}

#[tokio::test]
async fn the_keychain_entry_is_last_and_nothing_names_every_source() {
    if std::env::var("SATZ_STUDIO_KEYCHAIN_TEST").as_deref() != Ok("1") {
        println!("SATZ_STUDIO_KEYCHAIN_TEST is not 1: the keychain check is skipped");
        return;
    }
    let _lock = ENV.lock().await;
    let empty = tempfile::tempdir().expect("a temp dir");
    let path = empty.path().to_str().expect("utf-8").to_string();
    let _env = Env::set(&[
        ("ANTHROPIC_API_KEY", None),
        ("ANTHROPIC_AUTH_TOKEN", None),
        ("PATH", Some(&path)),
    ]);

    Credential::store_in_keychain("sk-example-keychain").expect("stores");
    let (credential, source) = Credential::resolve()
        .await
        .expect("resolves from the keychain");
    assert_eq!(
        credential,
        Credential::ApiKey("sk-example-keychain".to_string())
    );
    assert_eq!(source, CredentialSource::Keychain);

    Credential::delete_from_keychain().expect("deletes");
    Credential::delete_from_keychain().expect("deleting an absent entry is fine");
    let error = Credential::resolve().await.expect_err("nothing is left");
    let ClaudeError::NoCredential { tried } = &error else {
        panic!("{error}")
    };
    assert_eq!(tried.len(), 4, "{tried:?}");
    assert!(tried[2].contains("`ant` is not on PATH"));
    assert!(tried[3].contains("no keychain entry"));
    assert!(error.to_string().contains("ant auth login"));
    assert!(matches!(
        Credential::store_in_keychain(" "),
        Err(ClaudeError::Keychain(_))
    ));
}
