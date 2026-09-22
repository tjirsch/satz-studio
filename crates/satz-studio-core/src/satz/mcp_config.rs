//! `satz mcp-config <estate>` — the MCP client configuration this estate needs, as satz
//! renders it.
//!
//! `satz mcp` is the server an agentic client starts; the block that client reads to
//! start it is satz's own knowledge — the binary by absolute path, the root the server
//! may work under, the capability ceiling `satz mcp` parses — so the app asks satz for
//! it instead of assembling a second copy that drifts the day a flag changes.
//!
//! Run without `--write` it prints the block on stdout and its notes on stderr; run with
//! `--write` it merges satz's own key into the client's file and prints on stderr what
//! that came to. Every refusal is satz's, and reaches the operator as satz wrote it.

use std::fmt;

use super::{Allow, SatzCli, SatzError};

/// The client a configuration is for: what `--client` takes, and what a sentence calls
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Client {
    /// `.mcp.json`, read from the directory Claude Code starts in
    ClaudeCode,
    /// the `mcpServers` block of Claude Desktop's own configuration file
    ClaudeDesktop,
}

impl Client {
    /// Both, in the order the view shows them.
    pub const ALL: [Client; 2] = [Client::ClaudeCode, Client::ClaudeDesktop];

    /// The value satz takes after `--client`.
    pub fn as_arg(self) -> &'static str {
        match self {
            Client::ClaudeCode => "claude-code",
            Client::ClaudeDesktop => "claude-desktop",
        }
    }

    /// The client's name as a person writes it.
    pub fn label(self) -> &'static str {
        match self {
            Client::ClaudeCode => "Claude Code",
            Client::ClaudeDesktop => "Claude Desktop",
        }
    }
}

impl fmt::Display for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// What one run is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Run {
    /// print the configuration and the notes that go with it
    Show,
    /// write satz's key into the client's file, refusing one that is there with other
    /// arguments
    Write,
    /// write it over satz's own key, which only the refusal above asks for
    Replace,
}

/// The argument vector after `satz --config <dir>`: `mcp-config <estate> --client
/// <client> --allow <ceiling>`, and the flags the run asks for. The root is satz's —
/// it takes the estate's own directory, the one `--config` names — and the binary is
/// the satz that runs, resolved by satz itself.
pub fn args(estate: &str, client: Client, allow: Allow, run: Run) -> Vec<String> {
    let mut argv = vec![
        "mcp-config".to_string(),
        estate.to_string(),
        "--client".to_string(),
        client.as_arg().to_string(),
        "--allow".to_string(),
        allow.as_arg().to_string(),
    ];
    match run {
        Run::Show => {}
        Run::Write => argv.push("--write".to_string()),
        Run::Replace => argv.extend(["--write".to_string(), "--force".to_string()]),
    }
    argv
}

/// What satz printed: the block on stdout, the notes or the line about the write on
/// stderr, neither reworded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Printed {
    pub stdout: String,
    pub stderr: String,
}

/// Run `satz mcp-config` and hand back what it printed. A refusal is
/// [`SatzError::Exit`], carrying satz's own stderr.
pub async fn run(
    cli: &SatzCli,
    estate: &str,
    client: Client,
    allow: Allow,
    run: Run,
) -> Result<Printed, SatzError> {
    let out = cli.output(&args(estate, client, allow, run)).await?;
    Ok(Printed {
        stdout: out.stdout,
        stderr: out.stderr,
    })
}

/// The last thing satz said, which for a write is what the write came to — created,
/// added, replaced or unchanged. satz opens every run with its version banner and puts
/// its notes above this line, so a toast carries this and the card carries all of it.
/// Empty only for a run that said nothing at all.
pub fn outcome(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// What a run that did not succeed says, as the view puts it on screen: satz's own
/// stderr where satz refused, word for word, and the app's error where satz never ran
/// or never spoke.
pub fn refusal(e: &SatzError) -> String {
    match e {
        SatzError::Exit { stderr, .. } if !stderr.trim().is_empty() => stderr.clone(),
        other => other.to_string(),
    }
}

/// Whether the refusal in hand is the one `--force` answers: satz says so itself, in
/// the sentence it prints about the key that is already there with other arguments.
/// Every other refusal — a file satz cannot parse, a name it cannot derive — is shown
/// and nothing is offered, because `--force` would not change it.
pub fn force_would_answer(refusal: &str) -> bool {
    refusal.contains("--force replaces it")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_button_is_one_satz_run_over_the_open_estate() {
        assert_eq!(
            args(
                "C0example.satz",
                Client::ClaudeCode,
                Allow::ReadWrite,
                Run::Show
            ),
            [
                "mcp-config",
                "C0example.satz",
                "--client",
                "claude-code",
                "--allow",
                "read,write"
            ]
        );
        assert_eq!(
            args(
                "C0example.satz",
                Client::ClaudeDesktop,
                Allow::Read,
                Run::Write
            ),
            [
                "mcp-config",
                "C0example.satz",
                "--client",
                "claude-desktop",
                "--allow",
                "read",
                "--write"
            ]
        );
        assert_eq!(
            args(
                "C0example.satz",
                Client::ClaudeCode,
                Allow::ReadWriteExec,
                Run::Replace
            ),
            [
                "mcp-config",
                "C0example.satz",
                "--client",
                "claude-code",
                "--allow",
                "read,write,exec",
                "--write",
                "--force"
            ]
        );
    }

    /// The ceiling Settings holds is written out on every run, and `--force` is never on
    /// a run the operator did not ask for it on.
    #[test]
    fn the_ceiling_is_always_passed_and_force_only_on_a_replace() {
        for allow in [Allow::Read, Allow::ReadWrite, Allow::ReadWriteExec] {
            for client in Client::ALL {
                for run in [Run::Show, Run::Write, Run::Replace] {
                    let argv = args("C0example.satz", client, allow, run);
                    let at = argv.iter().position(|a| a == "--allow").expect("--allow");
                    assert_eq!(argv[at + 1], allow.as_arg());
                    assert_eq!(
                        argv.contains(&"--force".to_string()),
                        run == Run::Replace,
                        "{argv:?}"
                    );
                    assert_eq!(
                        argv.contains(&"--write".to_string()),
                        run != Run::Show,
                        "{argv:?}"
                    );
                }
            }
        }
    }

    /// satz's own refusal for a key that is already there with other arguments, as it
    /// prints it: the version banner it opens every run with, then the refusal.
    const KEY_DIFFERS: &str = "\
satz v0.77.0 (built 2026-09-23 00:10:56)
error: /estates/acme/.mcp.json already holds the server \"satz\", with other arguments:

{
  \"type\": \"stdio\",
  \"command\": \"/opt/bin/satz\",
  \"args\": [
    \"mcp\",
    \"--root\",
    \"/estates/acme\",
    \"--allow\",
    \"read\"
  ]
}

--force replaces it. Every other server in the file is untouched either way.
";

    /// What a write that went through says, as satz prints it.
    const WROTE: &str = "\
satz v0.77.0 (built 2026-09-23 00:10:56)
added satz-C0example in /home/example/.config/Claude/claude_desktop_config.json: satz mcp --root /estates/acme --allow read (1 other server(s) untouched)
";

    /// satz's own refusal for a file it cannot read, which `--force` does not answer.
    const UNREADABLE: &str = "/estates/acme/.mcp.json: not valid JSON (expected value at line 1 column 20) \
— satz merges its own server into this file and does not replace one it cannot read. \
Fix the file, or move it aside and run this again.\n";

    #[test]
    fn a_differing_key_is_the_one_refusal_force_answers() {
        assert!(force_would_answer(KEY_DIFFERS));
        assert!(!force_would_answer(UNREADABLE));
        assert!(!force_would_answer(""));
    }

    /// What satz refused with is what the operator reads: the same bytes, with nothing
    /// of the app's around them.
    #[test]
    fn a_refusal_is_shown_as_satz_wrote_it() {
        for text in [KEY_DIFFERS, UNREADABLE] {
            let e = SatzError::Exit {
                command: "mcp-config C0example.satz --client claude-code --allow read --write"
                    .to_string(),
                status: exit_status(2),
                stderr: text.to_string(),
            };
            assert_eq!(refusal(&e), text);
        }
    }

    /// A run that never reached satz has no stderr to show, so the app's own error is
    /// what is said — never an empty message.
    #[test]
    fn a_run_that_never_spoke_says_what_the_app_knows() {
        let silent = SatzError::Exit {
            command: "mcp-config C0example.satz".to_string(),
            status: exit_status(2),
            stderr: "  \n".to_string(),
        };
        assert!(refusal(&silent).contains("mcp-config C0example.satz"));
        let io = SatzError::Io {
            context: "running `satz mcp-config`".to_string(),
            source: std::io::Error::other("no such file"),
        };
        assert_eq!(refusal(&io), io.to_string());
    }

    /// The write's own line is the last one, under the banner satz opens with — that is
    /// the sentence the toast carries, while the card carries the whole of it.
    #[test]
    fn the_toast_carries_the_line_the_write_ended_on() {
        assert_eq!(
            outcome(WROTE),
            "added satz-C0example in /home/example/.config/Claude/claude_desktop_config.json: satz mcp --root /estates/acme --allow read (1 other server(s) untouched)"
        );
        assert_eq!(
            outcome(
                "satz v0.77.0 (built 2026-09-23 00:10:56)\nunchanged satz in /estates/acme/.mcp.json: it already runs satz mcp --allow read\n"
            ),
            "unchanged satz in /estates/acme/.mcp.json: it already runs satz mcp --allow read"
        );
        assert_eq!(outcome(""), "");
    }

    #[cfg(unix)]
    fn exit_status(code: i32) -> std::process::ExitStatus {
        std::os::unix::process::ExitStatusExt::from_raw(code << 8)
    }

    #[cfg(windows)]
    fn exit_status(code: i32) -> std::process::ExitStatus {
        std::os::windows::process::ExitStatusExt::from_raw(code as u32)
    }
}
