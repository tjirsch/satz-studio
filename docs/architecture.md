# satz-studio architecture

What satz-studio is built of and how its parts run, for people changing it. The
decisions behind the shape are in [`adr/`](adr/README.md). The scaffold (U1) ships the
module contract of every part; a section below names the unit that fills it where the
code is still a stub (a stub returns `Unimplemented`, never a panic).

## 1. Goal

satz-studio is a desktop app over [satz](https://github.com/tjirsch/satz). It runs the
interview satz's packs declare, edits the pack choices (the estate map) and the values
in the estate files through typed fields, runs satz commands, and drives satz through
Claude with the satz MCP tools as the model's tools. Version one edits what exists: an
answer, a pack choice, an attribute value. Adding and removing resources and blocks is
not in it.

## 2. Constraints

- **satz owns the estate.** The writers for answers and pack choices
  (`satz_interview`) and for import ids (`satz adopt`) are satz's; the app calls them and
  never re-implements them. What the app writes itself is one value inside its span,
  checked by `satz transpile --check` before it replaces the file.
- **The app never touches `hcl_dir`.** The generated HCL is satz's output; the app reads
  the estate's `config.toml` to learn where it is and leaves it alone.
- **One identity per estate.** Every open estate has its own `satz mcp` child with the
  estate's directory as `--root`; the identity its live tools run as is what
  `satz_open` returned (`runs_as`), shown in the top bar and configured nowhere.
- **Fail fast, no degraded modes.** A satz older than `MIN_SATZ` is refused at startup;
  a settings file that does not parse is an error; a `schema_dir` without a schema is a
  blocking diagnostic and the Resources view is read-only until `satz update-schema`;
  a file that changed on disk under an edit is refused, never merged; a JSON field satz
  removed fails the deserialisation instead of defaulting.
- **Privacy.** The repository goes public with its history: example values only, satz's
  gate on every commit. Transcripts live under the app's data directory, credentials in
  the OS keychain, nothing of either inside an estate.
- **Offline tests.** Every unit builds and tests against `tests/fixtures` and the pinned
  `vendor/satz`; only the live Claude check needs a credential.

## 3. Building blocks

Two crates in one workspace, satz pinned once as the submodule `vendor/satz`
([ADR 0002](adr/0002-a-separate-repository-with-satz-pinned-once.md)).

### `crates/satz-studio-core`: the headless half

| module | responsibility | main public types |
|---|---|---|
| `src/estate.rs` | an estate directory as satz sees it: `config.toml` read into `ToolConfig` with satz's defaults, paths resolved against the config's directory, the `.satz` files that declare an `estate`, the loader that resolves `use "…"` (the file's own directory, then `include_dirs`), the resolved params | `EstateDir`, `ToolConfig`, `EstateError` |
| `src/cst/mod.rs`, `src/cst/grammar.rs` | the lossless document layer over the tree-sitter grammar of Satz ([ADR 0003](adr/0003-the-document-layer-is-the-tree-sitter-grammar.md)): byte spans, comments kept, `text()` is the file; `Cst::lower` hands the same text to `satz_core::satz::parse`, the authority on meaning; `scan_uses` finds every `use` line, active or commented; `render_value` writes a value as satz's own writer does | `Cst`, `Node`, `NodeKind`, `Span`, `UseLine`, `TypedValue`, `StyleCtx`, `scan_uses`, `render_value`, `grammar::language` |
| `src/edit/mod.rs` | the edit primitives and the write discipline (section 4b) | `Edit`, `EditSession`, `Proposed`, `Committed`, `Rollback`, `Checker`, `CheckFailure` |
| `src/schema.rs` | the provider schema as `satz update-schema` writes it, the types lifted from satz `src/schema.rs`; the attribute type decoded for a typed field | `ResourceRegistry`, `AttrType`, `BlockSchema`, `AttributeSchema` |
| `src/model/mod.rs` | the view model, built pure and rebuilt after every commit, reload or questions refresh: the outline classified as satz classifies it, one row per attribute, param and pack; a key the schema does not know is `Unknown` and read-only, a gate without a line is `Absent`, a computed attribute is locked | `EstateModel`, `ResourceNode`, `AttrRow`, `ParamRow`, `PackRow`, `LineState`, `SourceValue`, `EditMode`, `SchemaStatus` |
| `src/satz/binary.rs` | where satz is and which version: the Settings override, `PATH`, `~/.local/bin/satz`; the gate against `MIN_SATZ` | `SatzBinary`, `MIN_SATZ` |
| `src/satz/cli.rs` | `satz --config <dir> <args…>` with stdout and stderr streamed line by line, and `--format json` output typed; for what MCP does not serve (`update-schema`, `hcl-init`, `plan`, `init`) | `SatzCli`, `CliLine` |
| `src/satz/mcp.rs` | one `satz mcp` child per estate, spoken to with rmcp over stdio; every rmcp type stays inside this file | `McpSession`, `ToolInfo`, `ToolAnnotations`, `ToolOutcome` |
| `src/satz/session.rs` | one session per open estate: the CLI runner, the MCP child, the write lock every writer takes, the identity from `satz_open`; `apply` and `bootstrap` as a one-shot script in the OS terminal | `EstateSession` |
| `src/satz/reports.rs` | serde mirrors of what satz prints with `--format json` and returns as `structuredContent`: unknown fields ignored, missing required fields fail; each round-trips a recorded output of the pinned satz | `QuestionsReport`, `QuestionRow`, `InterviewArgs`, `InterviewReport`, `OpenReport`, `EstatesReport`, `CompileSummary` |
| `src/satz/mod.rs` | the capability ceiling and the one error type of the driver | `Allow`, `SatzError` |
| `src/llm/mod.rs` | Claude natively: the Messages API wire types as the app's only message model, the streaming client, the agent loop with the MCP tools bridged as Claude tools, credentials, and the trait other providers implement by mapping into this model | `Request`, `Response`, `Message`, `ContentBlock`, `ToolDef`, `StreamEvent`, `ChatProvider`, `ClaudeClient`, `Credential`, `Agent`, `AgentEvent`, `Approval`, `tool_defs` |
| `src/transcript.rs` | conversations as JSONL under `<data dir>/satz-studio/transcripts/<sha256 of the estate path>/`, outside the estate | `TranscriptStore`, `Transcript` |
| `src/settings.rs` | `<config dir>/satz-studio/settings.toml`: a missing file is the first run, a broken one is an error; no credential in it | `Settings`, `ProviderChoice`, `Theme` |
| `src/diag.rs` | the one diagnostic type, and the parser for what satz prints (`file:line: msg`, `satz: line N: msg`, the severity prefixes, the banner dropped, an indented line continuing the one above) | `Diagnostic`, `Severity`, `DiagSource`, `parse_satz_output` |

`build.rs` compiles `vendor/satz-tree-sitter/src/parser.c` into the crate.

### `crates/satz-studio`: the window

Dioxus 0.7 on the webview renderer
([ADR 0001](adr/0001-dioxus-desktop-on-the-webview-renderer.md)). `src/main.rs` opens
the window and shows the boot page. The shell (navigation rail, top bar, diagnostics
drawer, snackbar), the stores and the views (Estates, Interview, Params, Map, Resources,
Commands, Chat, Settings) are U7, U8 and U9. Every side effect goes through one
coroutine per open estate that owns the `Arc<EstateSession>`; the stores are written
from there only, and nothing blocks in an event handler. `assets/css/` carries the
Material 3 Expressive tokens and component anatomies as CSS; the Material Symbols font
is bundled with the app.

## 4. Runtime views

### 4a. Opening an estate (U4, U5)

1. The Estates view walks a folder with `EstateDir::discover`: every `config.toml` down
   to depth 6, at most 200, skipping `hcl/`, `target/`, `evidence/`, `node_modules/` and
   dot-directories.
2. `EstateDir::open` reads the `config.toml` into `ToolConfig` and resolves its paths
   against the config's directory; `estates()` lists the `.satz` files in `yaml_dir`
   whose text declares an `estate`.
3. `SatzBinary::locate` finds satz and holds it to `MIN_SATZ`; `SatzError::TooOld` is a
   full-screen refusal naming the version found and `satz self-update`.
4. `EstateSession::open(bin, dir, main, allow)` spawns `satz mcp --root <dir> --allow
   <allow>` through `McpSession::open`: initialize, keep the server's `instructions`,
   list the tools, read `satz://guide`, call `satz_open {config, estate}` and keep its
   `OpenReport` with `runs_as` and `deployment_mode`. `allow` is the Settings ceiling,
   `read,write` by default; `exec` lets `satz_scan_checkov` run Checkov. The child's
   stderr is a tab in Commands; a child exit is a visible error and a restart.
5. The model is built: `Cst::parse` of the main file, `EstateDir::params` for the
   resolved params (no schema needed), `satz_questions` through the session for the
   `QuestionsReport`, `ResourceRegistry::load_all(schema_dir)` or `SchemaStatus::Missing`,
   then `EstateModel::build`. The same build runs after every commit, reload and answer.

### 4b. The write discipline (U3)

Every writer takes `EstateSession::write_lock` first and holds it across the check and
the rename: a view edit, an interview answer, an agent's write tool.

A value edit, `Edit::ReplaceValue` or `Edit::ReplaceParam`, as `src/edit/mod.rs`
documents it:

1. `EditSession::open(path)` records the bytes, their sha256 and the `Cst`.
2. `apply(edits)` builds the new text in memory. `satz_core::satz::parse` must accept
   it, and the AST must differ from the old one only at the edited node; otherwise
   `EditError::Syntax` or `EditError::ChangedElsewhere`, and nothing is written.
3. `Proposed::commit(&dyn Checker)`:
   - refuse if the sha256 on disk differs from the snapshot: `Rollback::ChangedOnDisk`,
     no merge;
   - write `<name>.satz.studio-tmp` beside the file, so `use` and `include_dirs`
     resolve identically;
   - `Checker::check(tmp)`: `satz_transpile_check {estate: <tmp>}` over the session in
     the app, `satz --config <dir> transpile <tmp> --check` in the verification harness;
     both must agree. A refusal deletes the temp file and returns
     `Rollback::Check(diagnostics)` with every diagnostic re-pointed from the temp name
     to the real file (`Diagnostic::repoint`); no diagnostic names the temp file;
   - atomic rename over the real file; re-read and verify the hash. `Committed { path,
     sha256, summary }` is the next session's snapshot.

A delegated write (an answer, a pack toggle, a `oneof` choice) is `satz_interview
{answers}` or `{accept_defaults}`: satz's own writer on the real file. The app snapshots
the bytes first, calls the tool, runs the check on the real path, and on failure writes
the snapshot back. A `notify` watcher on `yaml_dir`, ignoring `*.studio-tmp`, reloads
the estate on an external change.

### 4c. An agent turn (U6, U9)

`Agent::run_turn(user_text, events, cancel)` over one `EstateSession`:

- **Tools.** `tool_defs(session.tools())` turns every MCP `ToolInfo` into a `ToolDef`:
  `input_schema` verbatim, the output schema's top-level keys appended to the
  description, sorted by name for a stable cache prefix.
- **System prompt.** `system[0]` is the studio preamble (edit `.satz` through the satz
  tools, never `hcl/`; run `satz_transpile_check` after every write; never invent an
  id), then the session's `instructions`, then the text of `satz://guide`, with
  `cache_control: ephemeral`. `system[1]` is the volatile estate context (path,
  `runs_as`, `deployment_mode`, the questions summary, up to 20 diagnostics, the
  outline) with no breakpoint. A second breakpoint sits on the last user message's last
  block. The render order is tools, system, messages: the stable part first.
- **Stream.** `ChatProvider::stream` yields `StreamEvent`s, assembled into a `Response`;
  the assistant message is appended unchanged, thinking blocks and their signatures
  included, so the next request echoes what the API returned.
- **Tool calls.** On `StopReason::ToolUse` every `ToolUse` block of the message is
  executed in order under the write lock. Approval reads the tool's annotations: a
  `read_only` tool runs without asking; anything else raises
  `AgentEvent::ToolCallPending` and waits for `Approval::Once`, `ForSession` or `Deny`;
  a `destructive` tool always asks; `auto_approve_writes` skips the card for
  non-destructive tools. `Deny` returns a `ToolResult` with `is_error: true` saying the
  operator denied it, so the model can continue. All results of one assistant message
  go back in one user message, and the loop runs again.
- **Ends.** `EndTurn` is `TurnDone`. `MaxTokens` is shown and never continued
  automatically. `Refusal` discards the partial turn and raises `AgentEvent::Refused`
  with the category, the explanation and the recommended model when `stop_details`
  names one; server-side fallbacks are on by default. `PauseTurn` loops once more.
  Cancellation aborts the request and discards the partial turn; a tool call in flight
  completes and its result is dropped.
- **Credential.** `Credential::resolve` tries `ANTHROPIC_API_KEY`, then
  `ANTHROPIC_AUTH_TOKEN`, then `ant auth print-credentials` when `ant` is on `PATH`,
  then the keychain entry Settings writes; `ClaudeError::NoCredential` names the four.
  Settings shows which one won.
- **Other providers** implement `ChatProvider` by mapping the Claude-shaped `Request`
  to their wire format and their stream back into `StreamEvent`s; `Capabilities` says
  what a provider drops (thinking, effort, caching) and the Chat view shows it. Claude
  never passes through a mapping.
- **Transcripts.** `TranscriptStore` appends one message per line; a resume replays them
  on the same model; a model switch starts a new transcript.

## 5. Deployment

`dx bundle --release --platform desktop` per OS: `.dmg` and `.app` on macOS (arm64 and
x86_64), `.deb` and `.AppImage` on Linux, `.msi` on Windows with the WebView2 Evergreen
bootstrapper; a sha256 sidecar beside each. The bundle identity is in `Dioxus.toml`.
The bundles are unsigned in version one: macOS shows the Gatekeeper prompt and Windows
the SmartScreen one. The release workflow is U10.

CI while the repository is private runs on `ubuntu-24.04` for pull requests:
`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test -p satz-studio-core --locked` with the pinned satz release installed by its
cargo-dist installer, and the privacy gate. macOS and Windows jobs run on tags and by
manual dispatch until the repository is public, then on every pull request. On Windows
the satz binary is built from the submodule until satz ships a Windows release.

## 6. Decisions

| record | decision |
|---|---|
| [0001](adr/0001-dioxus-desktop-on-the-webview-renderer.md) | Dioxus 0.7 desktop on the webview renderer |
| [0002](adr/0002-a-separate-repository-with-satz-pinned-once.md) | a separate repository; satz pinned once, as the submodule; the binary required at `MIN_SATZ` |
| [0003](adr/0003-the-document-layer-is-the-tree-sitter-grammar.md) | the document layer is the tree-sitter grammar, vendored and compiled in; satz-core stays the authority on meaning |

Claude native with other providers adapting into its model, tool approval by MCP
annotations, `apply` and `bootstrap` in the user's terminal, pack rows derived from the
estate file, transcripts outside the estate, and refusal fallbacks on by default are
decided in the plan; each gets its record with the unit that builds it.

## 7. Not built, and why

- **No embedded terminal.** `apply` and `bootstrap` hand stdio to tofu and to the
  human; tofu's approval prompt is the safety step. `EstateSession::external_command`
  writes a one-shot script under the app's data directory and `open_in_terminal` opens
  it with the OS terminal; the app shows "running in your terminal" and reloads the
  estate on focus. `-auto-approve` is never passed.
- **No copy of satz's `PACK_LINES` table.** A pack row is derived from three sources
  the estate already has: `scan_uses` over the file, the questions report, and the
  resolved params. A gate with no line is `LineState::Absent`, and the row's action is
  `satz_merge_presets`, which is what satz tells an operator.
- **No second parser.** The document layer is the tree-sitter grammar and meaning is
  `satz_core::satz::parse` (ADR 0003). A grammar gap is fixed in the grammar repository.
- **No merge.** A file that changed on disk under an edit is refused and reloaded.
