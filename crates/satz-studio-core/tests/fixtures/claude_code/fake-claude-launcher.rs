//! The Windows stand-in for the shell shim around the fake Claude Code CLI.
//!
//! The app refuses a `.bat`/`.cmd` (`ClaudeCodeError::BatchFile`), because a turn hands
//! Claude Code a JSON MCP configuration and a multi-line system prompt and neither can be
//! quoted for cmd.exe. That refusal is the product's, so the tests honour it: on Windows
//! the fake is this executable, copied next to its `fake-claude.json`, rather than a batch
//! file. It sets `FAKE_CLAUDE_CONFIG` to the configuration beside itself and runs the same
//! `fake-claude.py` the unix shim runs, forwarding every argument.
use std::path::PathBuf;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let exe = std::env::current_exe().expect("the launcher knows its own path");
    let dir = exe.parent().expect("the launcher sits in a directory");
    let config = dir.join("fake-claude.json");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("claude_code")
        .join("fake-claude.py");
    let status = Command::new("python")
        .arg(&script)
        .args(std::env::args_os().skip(1))
        .env("FAKE_CLAUDE_CONFIG", &config)
        .status()
        .unwrap_or_else(|e| panic!("running {}: {e}", script.display()));
    // The fake's exit code is part of what the tests assert, so it is passed through.
    ExitCode::from(u8::try_from(status.code().unwrap_or(1)).unwrap_or(1))
}
