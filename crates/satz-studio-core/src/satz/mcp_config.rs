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
use std::path::PathBuf;

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

/// One server entry as satz renders it: the program the client starts and its
/// arguments, with the entry itself kept whole for comparing it with a file's.
#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    /// the key satz writes the entry under
    pub key: String,
    pub command: String,
    pub args: Vec<String>,
    /// the entry as JSON, every field satz wrote in it
    pub entry: serde_json::Value,
}

impl Server {
    /// Read the entry `key` holds from a JSON object of servers. `None` when the key is
    /// not there; an entry without a string `command` or a list of string `args` is not
    /// one satz writes, and is an error naming what is missing.
    fn read(
        key: &str,
        servers: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<Option<Server>, String> {
        let Some(entry) = servers.get(key) else {
            return Ok(None);
        };
        let command = entry["command"]
            .as_str()
            .ok_or_else(|| format!("the server `{key}` has no string `command`"))?
            .to_string();
        let args = serde_json::from_value::<Vec<String>>(entry["args"].clone())
            .map_err(|_| format!("the server `{key}` has no list of string `args`"))?;
        Ok(Some(Server {
            key: key.to_string(),
            command,
            args,
            entry: entry.clone(),
        }))
    }

    /// The value after `flag` in the arguments, when the flag is there.
    fn flag(&self, flag: &str) -> Option<&str> {
        let at = self.args.iter().position(|a| a == flag)?;
        self.args.get(at + 1).map(String::as_str)
    }

    /// The directory `satz mcp` is confined to: the value of `--root`.
    pub fn root(&self) -> Option<&str> {
        self.flag("--root")
    }

    /// The capability ceiling the server is started with: the value of `--allow`.
    pub fn allow(&self) -> Option<&str> {
        self.flag("--allow")
    }
}

/// The one server the block satz printed holds. satz prints exactly one key under
/// `mcpServers`; anything else is output this app does not read, and an error quoting
/// it.
pub fn server(block: &str) -> Result<Server, SatzError> {
    let unreadable = |reason: String| SatzError::Printed {
        command: "mcp-config".to_string(),
        reason: format!("{reason}:\n{block}"),
    };
    let doc: serde_json::Value = serde_json::from_str(block)
        .map_err(|e| unreadable(format!("the block is not JSON ({e})")))?;
    let servers = doc["mcpServers"]
        .as_object()
        .ok_or_else(|| unreadable("the block has no `mcpServers` object".to_string()))?;
    let mut keys = servers.keys();
    let (Some(key), None) = (keys.next(), keys.next()) else {
        return Err(unreadable(format!(
            "the block holds {} servers, not one",
            servers.len()
        )));
    };
    Server::read(key, servers)
        .map_err(unreadable)?
        .ok_or_else(|| unreadable(format!("the server `{key}` is not in the block")))
}

/// The directory `satz mcp` is confined to for this estate, as satz renders it: the
/// `--root` of the server it prints. This is the one rule for the root — satz's — used
/// for the app's own `satz mcp` and written into every client's configuration.
pub fn root(printed: &Printed) -> Result<PathBuf, SatzError> {
    let server = server(&printed.stdout)?;
    server
        .root()
        .map(PathBuf::from)
        .ok_or_else(|| SatzError::Printed {
            command: "mcp-config".to_string(),
            reason: format!("the server has no `--root`: {:?}", server.args),
        })
}

/// The file a `--write` puts the configuration in, as satz names it in the last of its
/// notes: `then: satz mcp-config … --write   # writes <file>` for Claude Code, `# merges
/// the key into <file>` for Claude Desktop. The path is satz's to derive — the estate's
/// directory for one client, the platform's own place for the other — so it is read,
/// never rebuilt; notes that name no file are an error quoting them.
pub fn target_file(notes: &str) -> Result<PathBuf, SatzError> {
    notes
        .lines()
        .filter(|l| l.trim_start().starts_with("then:"))
        .find_map(|l| {
            let (_, comment) = l.split_once(" --write   # ")?;
            let comment = comment.trim();
            comment
                .strip_prefix("writes ")
                .or_else(|| comment.strip_prefix("merges the key into "))
                .map(|p| PathBuf::from(p.trim()))
        })
        .ok_or_else(|| SatzError::Printed {
            command: "mcp-config".to_string(),
            reason: format!("the notes name no file a write goes to:\n{notes}"),
        })
}

/// What the client's file holds for this estate, against what satz printed.
#[derive(Debug, Clone, PartialEq)]
pub enum OnDisk {
    /// no file, or a file without satz's key: nothing is configured yet
    Absent,
    /// satz's key holds exactly the entry satz printed
    Same { allow: Option<String> },
    /// satz's key holds another entry — another ceiling, another binary, another root.
    /// `--write` refuses it and `--write --force` replaces it.
    Differs { allow: Option<String> },
    /// the file is there and satz's key cannot be read from it; the reason says why
    Unreadable(String),
}

/// The configuration a client reads for this estate: the file, satz's key in it, and
/// what that key holds against the entry satz printed for the ceiling in hand.
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    pub file: PathBuf,
    pub key: String,
    pub on_disk: OnDisk,
}

/// Read the client's file for the entry satz printed. The file and the key are satz's
/// (its notes and its block); what is compared is the entry as a whole, which is what
/// satz compares before it refuses a `--write`.
pub fn written(printed: &Printed) -> Result<Written, SatzError> {
    let server = server(&printed.stdout)?;
    let file = target_file(&printed.stderr)?;
    let on_disk = on_disk(&file, &server);
    Ok(Written {
        file,
        key: server.key,
        on_disk,
    })
}

fn on_disk(file: &std::path::Path, printed: &Server) -> OnDisk {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return OnDisk::Absent,
        Err(e) => return OnDisk::Unreadable(format!("{}: {e}", file.display())),
    };
    let doc: serde_json::Value = match serde_json::from_str(&text) {
        Ok(doc) => doc,
        Err(e) => return OnDisk::Unreadable(format!("{}: not valid JSON ({e})", file.display())),
    };
    let servers = match doc.get("mcpServers") {
        None => return OnDisk::Absent,
        Some(serde_json::Value::Object(servers)) => servers,
        Some(_) => {
            return OnDisk::Unreadable(format!(
                "{}: `mcpServers` is not a JSON object",
                file.display()
            ));
        }
    };
    match Server::read(&printed.key, servers) {
        Ok(None) => OnDisk::Absent,
        Ok(Some(there)) => {
            let allow = there.allow().map(str::to_string);
            if there.entry == printed.entry {
                OnDisk::Same { allow }
            } else {
                OnDisk::Differs { allow }
            }
        }
        Err(e) => OnDisk::Unreadable(format!("{}: {e}", file.display())),
    }
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

    /// What `satz mcp-config` prints for Claude Code without `--write`, as satz v0.81.0
    /// prints it: the block on stdout, the banner and the notes on stderr.
    const BLOCK: &str = r#"{
  "mcpServers": {
    "satz": {
      "type": "stdio",
      "command": "/opt/bin/satz",
      "args": [
        "mcp",
        "--root",
        "/estates/acme",
        "--allow",
        "read,write"
      ]
    }
  }
}
"#;
    const NOTES: &str = "\
satz v0.81.0 (built 2026-09-23 08:20:04)
note: the client starts this binary by absolute path, so it needs no PATH of its own: /opt/bin/satz
note: the server may work under /estates/acme and holds no estate until a call opens one — `satz_open` with C0example.satz
note: Claude Code reads .mcp.json from the directory it starts in, and asks before it starts a server it has not seen
then: satz mcp-config C0example.satz --client claude-code --write   # writes /estates/acme/.mcp.json
";

    fn printed() -> Printed {
        Printed {
            stdout: BLOCK.to_string(),
            stderr: NOTES.to_string(),
        }
    }

    #[test]
    fn the_block_is_read_as_one_server_with_its_root_and_ceiling() {
        let server = server(BLOCK).unwrap();
        assert_eq!(server.key, "satz");
        assert_eq!(server.command, "/opt/bin/satz");
        assert_eq!(server.root(), Some("/estates/acme"));
        assert_eq!(server.allow(), Some("read,write"));
        assert_eq!(root(&printed()).unwrap(), PathBuf::from("/estates/acme"));
    }

    /// A block of a shape satz does not print is an error quoting it, never a root
    /// guessed at.
    #[test]
    fn a_block_satz_does_not_print_is_refused() {
        for block in [
            "not json",
            "{}",
            r#"{"mcpServers": {}}"#,
            r#"{"mcpServers": {"a": {"command": "x", "args": []}, "b": {"command": "x", "args": []}}}"#,
            r#"{"mcpServers": {"satz": {"args": ["mcp"]}}}"#,
        ] {
            let e = server(block).unwrap_err();
            assert!(matches!(e, SatzError::Printed { .. }), "{block}: {e:?}");
        }
        let rootless = Printed {
            stdout: r#"{"mcpServers": {"satz": {"command": "x", "args": ["mcp"]}}}"#.to_string(),
            stderr: String::new(),
        };
        assert!(root(&rootless).unwrap_err().to_string().contains("--root"));
    }

    #[test]
    fn the_file_a_write_goes_to_is_the_one_satz_names() {
        assert_eq!(
            target_file(NOTES).unwrap(),
            PathBuf::from("/estates/acme/.mcp.json")
        );
        assert_eq!(
            target_file(
                "then: satz mcp-config C0example.satz --client claude-desktop --write   # merges the key into /home/example/.config/Claude/claude_desktop_config.json\n"
            )
            .unwrap(),
            PathBuf::from("/home/example/.config/Claude/claude_desktop_config.json")
        );
        assert!(matches!(
            target_file("note: nothing else\n").unwrap_err(),
            SatzError::Printed { .. }
        ));
    }

    /// The ceiling the agent runs at is the one in the client's file, compared entry by
    /// entry with the block satz printed for the setting.
    #[test]
    fn the_file_on_disk_is_compared_with_the_block() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".mcp.json");
        let mut p = printed();
        p.stderr = format!(
            "then: satz mcp-config C0example.satz --client claude-code --write   # writes {}\n",
            file.display()
        );
        let on = |p: &Printed| written(p).unwrap().on_disk;

        assert_eq!(on(&p), OnDisk::Absent);
        std::fs::write(
            &file,
            r#"{"mcpServers": {"other": {"command": "x", "args": []}}}"#,
        )
        .unwrap();
        assert_eq!(on(&p), OnDisk::Absent);

        std::fs::write(&file, BLOCK).unwrap();
        assert_eq!(
            on(&p),
            OnDisk::Same {
                allow: Some("read,write".to_string())
            }
        );
        let written_at_read = BLOCK.replace("\"read,write\"", "\"read\"");
        std::fs::write(&file, &written_at_read).unwrap();
        assert_eq!(
            on(&p),
            OnDisk::Differs {
                allow: Some("read".to_string())
            }
        );
        let written_without = BLOCK.replace(",\n        \"--allow\",\n        \"read,write\"", "");
        assert_ne!(written_without, BLOCK);
        std::fs::write(&file, &written_without).unwrap();
        assert_eq!(on(&p), OnDisk::Differs { allow: None });

        std::fs::write(&file, "{").unwrap();
        assert!(matches!(on(&p), OnDisk::Unreadable(r) if r.contains("not valid JSON")));
        std::fs::write(&file, r#"{"mcpServers": []}"#).unwrap();
        assert!(matches!(on(&p), OnDisk::Unreadable(_)));
        assert_eq!(written(&p).unwrap().key, "satz");
        assert_eq!(written(&p).unwrap().file, file);
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
