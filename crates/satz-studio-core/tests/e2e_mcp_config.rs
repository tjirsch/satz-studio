//! `satz mcp-config`, end to end with the installed satz over a copy of the smoke
//! estate: the run each button makes is one satz accepts, the block it prints is this
//! estate's own server, a write lands where the client reads it and says what it came
//! to, and the refusal for satz's key already there with other arguments reaches the app
//! as satz wrote it — with the run that answers it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use satz_studio_core::satz::mcp_config::{self, Client, Run};
use satz_studio_core::satz::{Allow, SatzBinary, SatzCli};

#[path = "fixtures/edit/support.rs"]
mod support;

const TIME_BOX: Duration = Duration::from_secs(60);

async fn cli(root: &Path) -> SatzCli {
    let bin = SatzBinary::locate(None).await.unwrap();
    SatzCli::new(bin, root.to_path_buf())
}

/// A path as a comparison can hold it on every platform: satz writes forward slashes
/// into the block, the app's own paths carry the platform's separator.
fn same_path(a: &str, b: &Path) -> bool {
    a.replace('\\', "/") == b.display().to_string().replace('\\', "/")
}

fn servers(block: &str) -> serde_json::Map<String, serde_json::Value> {
    let doc: serde_json::Value = serde_json::from_str(block)
        .unwrap_or_else(|e| panic!("the block is not JSON ({e}): {block}"));
    doc["mcpServers"]
        .as_object()
        .unwrap_or_else(|| panic!("no mcpServers object: {block}"))
        .clone()
}

fn args_of(server: &serde_json::Value) -> Vec<String> {
    serde_json::from_value(server["args"].clone()).expect("args are strings")
}

fn after(args: &[String], flag: &str) -> String {
    let at = args
        .iter()
        .position(|a| a == flag)
        .unwrap_or_else(|| panic!("{flag} is not in {args:?}"));
    args[at + 1].clone()
}

#[tokio::test]
async fn the_block_satz_prints_is_this_estates_own_server() {
    let copy = support::copy_smoke();
    let cli = cli(&copy.root).await;
    for client in Client::ALL {
        let printed = tokio::time::timeout(
            TIME_BOX,
            mcp_config::run(&cli, "smoke.satz", client, Allow::ReadWrite, Run::Show),
        )
        .await
        .expect("timed out")
        .unwrap();

        let servers = servers(&printed.stdout);
        assert_eq!(servers.len(), 1, "satz writes one key: {servers:?}");
        let (key, server) = servers.iter().next().unwrap();
        assert_eq!(
            key,
            match client {
                Client::ClaudeCode => "satz",
                Client::ClaudeDesktop => "satz-smoke",
            }
        );

        // the binary is the satz that printed the block, by absolute path
        let command = PathBuf::from(server["command"].as_str().expect("a command"));
        assert!(command.is_absolute(), "{command:?}");
        assert_eq!(
            command.file_stem().and_then(|s| s.to_str()),
            Some("satz"),
            "{command:?}"
        );

        let args = args_of(server);
        assert_eq!(args[0], "mcp");
        assert!(
            same_path(&after(&args, "--root"), &copy.root),
            "the root is not this estate's directory: {args:?}"
        );
        assert_eq!(after(&args, "--allow"), Allow::ReadWrite.as_arg());

        // what the block cannot say is on stderr, which is what the card shows under it
        assert!(printed.stderr.contains("note:"), "{}", printed.stderr);
    }
    assert!(
        !copy.root.join(".mcp.json").exists(),
        "a run without --write wrote a file"
    );
}

#[tokio::test]
async fn a_write_lands_then_is_unchanged_then_refuses_a_differing_ceiling() {
    let copy = support::copy_smoke();
    let cli = cli(&copy.root).await;
    let file = copy.root.join(".mcp.json");
    let write = async |allow: Allow, run: Run| {
        tokio::time::timeout(
            TIME_BOX,
            mcp_config::run(&cli, "smoke.satz", Client::ClaudeCode, allow, run),
        )
        .await
        .expect("timed out")
    };

    let created = write(Allow::ReadWrite, Run::Write).await.unwrap();
    assert!(file.is_file(), "{} was not written", file.display());
    let said = mcp_config::outcome(&created.stderr);
    assert!(said.starts_with("wrote "), "{said}");

    let again = write(Allow::ReadWrite, Run::Write).await.unwrap();
    let said = mcp_config::outcome(&again.stderr);
    assert!(said.starts_with("unchanged "), "{said}");

    // a lower ceiling is satz's own key with other arguments: refused, the file untouched
    let before = std::fs::read_to_string(&file).unwrap();
    let refusal = mcp_config::refusal(&write(Allow::Read, Run::Write).await.unwrap_err());
    assert!(refusal.contains(".mcp.json"), "{refusal}");
    assert!(
        mcp_config::force_would_answer(&refusal),
        "satz's refusal does not name the run that answers it: {refusal}"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);

    // "Replace it" is that run, and nothing else in the app passes --force
    let replaced = write(Allow::Read, Run::Replace).await.unwrap();
    let said = mcp_config::outcome(&replaced.stderr);
    assert!(said.starts_with("replaced "), "{said}");
    let servers = servers(&std::fs::read_to_string(&file).unwrap());
    assert_eq!(after(&args_of(&servers["satz"]), "--allow"), "read");
}
