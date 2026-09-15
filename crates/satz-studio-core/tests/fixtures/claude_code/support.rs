//! Shared by the `claude_code_*` tests: the fake CLI with one settings file per test,
//! the recorded streams, and the estate the sessions run on. Each test file includes
//! it with `#[path = "fixtures/claude_code/support.rs"]`.
//!
//! Nothing here touches the test process's environment: a test writes its own settings
//! file and a wrapper script that names it, so tests running at the same time never
//! see each other's.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use satz_studio_core::estate::EstateDir;
use satz_studio_core::llm::claude_code::ClaudeCodeCli;
use satz_studio_core::satz::{Allow, EstateSession, SatzBinary};

pub const TIME_BOX: std::time::Duration = std::time::Duration::from_secs(120);

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// A temporary directory on the REPOSITORY's drive rather than the system one. A
/// session's root is the longest common prefix of the estate's directory and every
/// directory its config names, and those reach into `vendor/satz`; on Windows the
/// system temporary directory is often on another drive, where a temporary estate and
/// the submodule share no prefix at all and there is no root to confine `satz mcp` to.
pub fn scratch() -> tempfile::TempDir {
    let dir = repo_root().join("target").join("test-scratch");
    std::fs::create_dir_all(&dir).unwrap();
    tempfile::Builder::new()
        .prefix("estate")
        .tempdir_in(&dir)
        .unwrap()
}

pub fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("claude_code")
}

/// One recorded stream by name (`text-turn`, `tool-turn`, …).
pub fn script(name: &str) -> String {
    fixtures()
        .join(format!("{name}.jsonl"))
        .display()
        .to_string()
}

/// `first.admin@example.com`, signed in through claude.ai — the only account address
/// this repository ever carries.
pub fn signed_in() -> serde_json::Value {
    serde_json::json!({
        "loggedIn": true,
        "authMethod": "claude.ai",
        "apiProvider": "firstParty",
        "email": "first.admin@example.com",
        "subscriptionType": "max",
    })
}

pub fn signed_out() -> serde_json::Value {
    serde_json::json!({"loggedIn": false, "authMethod": null, "apiProvider": null})
}

/// The fake CLI, set up for one test: its settings written into `dir`, and a wrapper
/// script that runs the fake with them. The wrapper is what the app is given as the
/// Claude Code binary.
pub struct Fake {
    pub path: PathBuf,
    pub dir: PathBuf,
}

impl Fake {
    /// The argv the app spawned the session with, once it has.
    pub fn args(&self) -> Vec<String> {
        let text =
            std::fs::read_to_string(self.dir.join("args.json")).expect("the session spawned");
        serde_json::from_str(&text).expect("the argv is JSON")
    }

    /// Every line the app wrote to the CLI's stdin, parsed.
    pub fn stdin(&self) -> Vec<serde_json::Value> {
        let path = self.dir.join("stdin.jsonl");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Vec::new();
        };
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("the client writes JSON"))
            .collect()
    }

    pub async fn locate(&self) -> ClaudeCodeCli {
        within(ClaudeCodeCli::locate(Some(&self.path)))
            .await
            .expect("the fake CLI is there")
    }
}

/// Set the fake up in `dir`: `auth` is what `auth status` answers, `scripts` are the
/// recorded streams it replays, one per user turn.
pub fn fake(dir: &Path, auth: serde_json::Value, scripts: &[&str]) -> Fake {
    fake_with(dir, auth, scripts, serde_json::Map::new())
}

/// The same, with extra settings folded into the file (`mcp_status`, `version`, …).
pub fn fake_with(
    dir: &Path,
    auth: serde_json::Value,
    scripts: &[&str],
    extra: serde_json::Map<String, serde_json::Value>,
) -> Fake {
    let config = dir.join("fake-claude.json");
    let mut settings = serde_json::Map::new();
    settings.insert("auth".to_string(), auth);
    settings.insert(
        "scripts".to_string(),
        serde_json::Value::Array(
            scripts
                .iter()
                .map(|s| serde_json::Value::String(script(s)))
                .collect(),
        ),
    );
    settings.insert(
        "args_out".to_string(),
        serde_json::Value::String(dir.join("args.json").display().to_string()),
    );
    settings.insert(
        "stdin_out".to_string(),
        serde_json::Value::String(dir.join("stdin.jsonl").display().to_string()),
    );
    settings.extend(extra);
    std::fs::write(
        &config,
        serde_json::to_string_pretty(&serde_json::Value::Object(settings)).unwrap(),
    )
    .unwrap();

    let fake = fixtures().join("fake-claude.py");
    let path = if cfg!(windows) {
        // Not a `.cmd`: the app refuses a batch file by design, because a turn's JSON MCP
        // configuration and multi-line system prompt cannot be quoted for cmd.exe. The
        // fake is the `fake-claude` launcher binary instead, copied beside the
        // `fake-claude.json` it reads.
        let shim = dir.join("claude.exe");
        std::fs::copy(env!("CARGO_BIN_EXE_fake-claude"), &shim).unwrap();
        shim
    } else {
        let shim = dir.join("claude");
        std::fs::write(
            &shim,
            format!(
                "#!/bin/sh\nFAKE_CLAUDE_CONFIG=\"{}\" exec python3 \"{}\" \"$@\"\n",
                config.display(),
                fake.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        shim
    };
    Fake {
        path,
        dir: dir.to_path_buf(),
    }
}

/// satz's own smoke estate, read straight from the pinned submodule, copied so a
/// session may open it.
pub struct SmokeCopy {
    _dir: tempfile::TempDir,
    pub root: PathBuf,
}

pub fn copy_smoke() -> SmokeCopy {
    let dir = scratch();
    let root = dir.path().canonicalize().unwrap();
    let yaml = root.join("yaml");
    std::fs::create_dir_all(&yaml).unwrap();
    let vendor = repo_root().join("vendor").join("satz");
    let source = vendor.join("tests").join("smoke").join("yaml");
    for entry in std::fs::read_dir(&source).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("satz") {
            std::fs::copy(&path, yaml.join(path.file_name().unwrap())).unwrap();
        }
    }
    let config = format!(
        "yaml_dir = \"yaml\"\nhcl_dir = \"hcl\"\ninclude_dirs = [\".\", \"yaml\", '{}']\nschema_dir = '{}'\npresets_dir = '{}'\ntf_tool = \"tofu\"\nprovider_version = \"7.14.1\"\n",
        vendor.display(),
        vendor.join("tests").join("schemas").display(),
        vendor.join("presets").display(),
    );
    std::fs::write(root.join("config.toml"), config).unwrap();
    SmokeCopy { _dir: dir, root }
}

impl SmokeCopy {
    /// A session on `smoke.satz`, with write capability.
    pub async fn open(&self) -> Arc<EstateSession> {
        let bin = SatzBinary::locate(None).await.unwrap();
        let dir = EstateDir::open(&self.root).unwrap();
        within(EstateSession::open(
            &bin,
            dir,
            PathBuf::from("smoke.satz"),
            Allow::ReadWrite,
        ))
        .await
        .unwrap()
    }
}

/// The satz binary the app gives Claude Code's MCP server.
pub async fn satz_binary() -> PathBuf {
    SatzBinary::locate(None).await.unwrap().path
}

/// `f` within the time box every async test keeps.
pub async fn within<F: Future>(f: F) -> F::Output {
    tokio::time::timeout(TIME_BOX, f)
        .await
        .expect("finished within the time box")
}
