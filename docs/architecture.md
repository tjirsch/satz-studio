# satz-studio architecture

What satz-studio is built of and how its parts run, for people changing it. The
decisions behind the shape are in [`adr/`](adr/README.md). Where a part is not built,
the section names the unit that builds it.

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
- **One identity per estate.** Every open estate has its own `satz mcp` child; the
  identity its live tools run as is what `satz_open` returned (`runs_as`), shown in the
  top bar and configured nowhere.
- **Fail fast, no degraded modes.** A satz older than `MIN_SATZ` is refused at startup;
  a settings file that does not parse is a full-screen refusal; a `schema_dir` without
  a schema is `SchemaStatus::Missing`; a file that changed on disk under an edit is
  refused, never merged; a JSON field satz removed fails the deserialisation.
- **Privacy.** The repository is public with its history: example values only, satz's
  gate on every commit. Transcripts live under the app's data directory, credentials in
  the OS keychain, nothing of either inside an estate.
- **Offline tests.** Every unit tests against `tests/fixtures` and the pinned
  `vendor/satz`; the tests that drive the `satz` binary need it installed; the live
  Claude check runs only with `SATZ_STUDIO_LIVE=1` and a credential.

## 3. Building blocks

Two crates in one workspace, satz pinned once as the submodule `vendor/satz`
([ADR 0002](adr/0002-a-separate-repository-with-satz-pinned-once.md)).

### `crates/satz-studio-core`: the headless half

| module | responsibility | main public types |
|---|---|---|
| `src/estate.rs` | an estate directory as satz sees it: `config.toml` read into `ToolConfig` with satz's defaults and resolved against its own directory; `discover` walks a folder for every `config.toml` (depth 6, at most 200, skipping `hcl/`, `target/`, `evidence/`, `node_modules/` and dot-directories); `estates` lists the `.satz` files in `yaml_dir` that declare an `estate`, skipping a checked temp file (`is_checked_temp`); `loader` resolves `use "…"` as satz does (the file's directory, then `include_dirs`); `params` and `deployment_mode` read the resolved params without a schema | `EstateDir`, `ToolConfig`, `EstateError`, `declares_an_estate` |
| `src/cst/` | the lossless document layer over the vendored tree-sitter grammar ([ADR 0003](adr/0003-the-document-layer-is-the-tree-sitter-grammar.md)); `grammar.rs` exposes the compiled parser, `build.rs` walks the tree into nodes with byte spans, `uses.rs` and `render.rs` read the pack lines and write values | `Cst`, `Node`, `NodeKind`, `Span`, `UseLine`, `UseState`, `TypedValue`, `StyleCtx`, `scan_uses`, `render_value`, `style_of`, `grammar::language` |
| `src/edit/` | the edit primitives and the write discipline (section 4b): `apply.rs` the splice and its proof, `commit.rs` the temp file and the rename, `check.rs` the two checkers, `snapshot.rs` the delegated write | `Edit`, `EditSession`, `Proposed`, `Committed`, `Rollback`, `Checker`, `CheckFailure`, `McpChecker`, `CliChecker`, `Snapshot`, `sha256_hex` |
| `src/schema.rs` | the provider schema as `satz update-schema` writes it, the types lifted from satz's `src/schema.rs`; `load_all` reads every `*.json` in `schema_dir` and is `SchemaError::Missing` for a directory that is absent or holds no resource type; `AttrType` decodes Terraform's type expression and prints it in Terraform's spelling | `ResourceRegistry`, `AttrType`, `BlockSchema`, `AttributeSchema`, `SchemaError` |
| `src/model/` | the view model, built pure and rebuilt after every commit and reload: `outline.rs` classifies the blocks as satz's `EstateResolver` and `is_child` do, `params.rs` joins the `params { }` block with the questions, `packs.rs` derives the pack rows ([ADR 0007](adr/0007-pack-rows-are-derived-from-the-estate-file.md)), `value.rs` decodes a string as satz's lexer reads it | `EstateModel`, `ResourceNode`, `ResourceKind`, `AttrRow`, `ParamRow`, `PackRow`, `PackRowKind`, `LineState`, `Choice`, `SourceValue`, `StrPart`, `EditMode`, `SchemaStatus` |
| `src/satz/binary.rs` | where satz is and which version: the Settings override, `PATH`, `~/.local/bin/satz`; the gate against `MIN_SATZ` | `SatzBinary`, `MIN_SATZ` |
| `src/satz/cli.rs` | `satz --config <dir> <args…>` in the estate's directory, stdout and stderr streamed line by line and cancellable; `json` types `--format json` output | `SatzCli`, `CliLine` |
| `src/satz/mcp.rs` | one `satz mcp` child per estate, spoken to with rmcp over stdio; every rmcp type stays inside this file | `McpSession`, `ToolInfo`, `ToolAnnotations`, `ToolOutcome` |
| `src/satz/session.rs` | one session per open estate: the CLI runner, the MCP child, the write lock every writer takes, the identity from `satz_open`; `apply` and `bootstrap` as a one-shot script in the OS terminal | `EstateSession`, `session_root` |
| `src/satz/reports.rs` | serde mirrors of what satz prints with `--format json` and returns as `structuredContent`: unknown fields ignored, missing required fields fail; the questions report round-trips a recorded output of the pinned satz. `Finding` is satz's own list of what the compile found after the front end — a `CompileSummary` carries the warnings and notes it did not refuse on, a `Refusal` the ones it did; `kind` is the kebab-case word satz writes, kept as a `String` so a kind satz adds is carried instead of failing the result | `QuestionsReport`, `QuestionRow`, `InterviewArgs`, `InterviewReport`, `OpenReport`, `EstatesReport`, `CompileSummary`, `Finding`, `FindingSeverity`, `Refusal` |
| `src/satz/mod.rs` | the capability ceiling and the one error type of the driver | `Allow`, `SatzError` |
| `src/llm/claude/` | Claude natively ([ADR 0004](adr/0004-claude-natively-other-providers-adapt-into-its-message-model.md)): `types.rs` the Messages API wire types as the app's only message model and `body()`, `sse.rs` the event-stream decoder and the assembler, `client.rs` the HTTPS client, `error.rs` the one error type | `Request`, `Response`, `Message`, `ContentBlock`, `SystemBlock`, `ToolDef`, `StopReason`, `StopDetails`, `Usage`, `Effort`, `ClaudeClient`, `ClaudeError` |
| `src/llm/agent/` | the agent loop over a `ToolHost`, the approval gate, and `bridge.rs`: MCP tools as Claude tool definitions and outcomes back as `tool_result` blocks | `Agent`, `AgentEvent`, `Approval`, `ToolHost`, `EstateContext`, `tool_defs`, `tool_result` |
| `src/llm/auth.rs` | where a credential comes from; the keychain entry `satz-studio` / `anthropic-api-key` | `Credential`, `CredentialSource` |
| `src/llm/provider/` | the providers that are not Claude, mapping the Claude-shaped request into their wire format and their stream back; `Capabilities` says what each drops | `ChatProvider`, `StreamEvent`, `Capabilities`, `OpenAiCompat`, `Ollama` |
| `src/transcript.rs` | conversations as JSONL under the app's data directory, outside the estate ([ADR 0008](adr/0008-transcripts-live-outside-the-estate.md)) | `TranscriptStore`, `Transcript`, `TranscriptHeader` |
| `src/settings.rs` | `<config dir>/satz-studio/settings.toml`: a missing file is the first run, a broken one is an error; no credential in it; `data_dir` is where transcripts and the one-shot scripts go | `Settings`, `ProviderChoice`, `Theme`, `settings_path`, `data_dir` |
| `src/diag.rs` | the one diagnostic type: `Diagnostic::from_finding` turns one of satz's findings into it — the severity mapped, the `kind` carried, a relative file resolved against the estate's directory, the group's header in front of the message — and `parse_satz_output` reads what satz prints when there is no finding to read (`file:line: msg`, `satz: line N: msg`, the severity prefixes, the banner dropped, an indented line continuing the one above) | `Diagnostic`, `Severity`, `DiagSource`, `parse_satz_output` |

`build.rs` compiles `vendor/satz-tree-sitter/src/parser.c` (and `scanner.c` when the
grammar has one) with `cc` into the crate; `COMMIT` beside it names the grammar commit,
and `scripts/sync-grammar.sh` refreshes the copy from a clean checkout.

`Cst::parse` never fails on text satz accepts. Text satz rejects still yields a tree, an
`ERROR` or `MISSING` token becoming an `Error` node where it stands, so a broken file
still shows; the `Err` case is a node kind the vendored grammar produces and the walk
does not map. `Cst::lower` hands the same text to `satz_core::satz::parse`, the
authority on meaning. `scan_uses` yields every `use` line: the active ones from `Use`
nodes, the commented ones from comment nodes in the exact shape satz's `pack_line`
writes (`// use "<path>"[ as <key>][ when <gate>]`, nothing else on the line), each
with the comment block above it as its phase. `render_value` escapes as satz's lexer
reads: `\` as `\\`, `"` as `\"`, a newline as `\n`, `{` as `{{` and `}` as `}}`, because
satz-core reads a doubled `}` as one brace.

### `crates/satz-studio`: the window

Dioxus 0.7 on the webview renderer
([ADR 0001](adr/0001-dioxus-desktop-on-the-webview-renderer.md)); `docs/ui.md` is the
window's own page. `src/main.rs` opens the window; `src/app.rs` loads `Settings` (a
file that does not parse is the `Fatal` page), provides the `AppStore` and starts the
app coroutine. State is two `#[derive(Store)]` roots in `src/state/mod.rs`, `AppStore`
and the `EstateStore` inside it, which is reset when an estate opens or closes; the
fields are listed in `docs/ui.md`. Every side effect runs in one of two coroutines and
the stores are written from there only:

- **the app coroutine** (`src/state/app_actions.rs`, `AppAction`): `LocateSatz`,
  `Discover`, `OpenEstate`, `CloseEstate`, `SaveSettings`, `ResolveCredential`,
  `StoreKey`; it locates satz at startup and walks `last_root`;
- **one coroutine per open estate** (`src/state/estate_actions.rs`, `EstateAction`),
  started by `EstateHost` in `src/shell/mod.rs` with the `Arc<EstateSession>` and
  living as long as the estate is open: `Reload`, `RunCommand`, `CancelCommand`,
  `RunTool`, `OpenInTerminal`, `Close`.

The shell (`src/shell/`) is the navigation rail with its badges, the top bar with the
`runs_as`, deployment-mode, schema and satz-version chips, the `SatzBanner` while satz
is missing or too old, the diagnostics drawer and the snackbar host. The views that
exist (`src/views/`) are Estates, Settings, Commands (`PALETTE`, fourteen commands with
typed arguments, `apply` and `bootstrap` marked `external` and offered as a command
line to copy or open in the terminal, `SESSION_TOOLS` as one click each) and Gallery;
Interview, Params, Map, Resources and Chat render `NotBuilt` — U8 builds the estate
views, U9 the chat.

## 4. Runtime views

### 4a. Opening an estate

1. The Estates view walks a folder with `EstateDir::discover`; each `config.toml` is
   opened and its estates listed with their `deployment_mode`, on a blocking thread.
2. `SatzBinary::locate(override)` takes the Settings path when it is set (a path that
   does not exist is `SatzError::NotFound` naming it), else `satz` on `PATH`, else
   `~/.local/bin/satz`; the first candidate that exists is run with `--version` and
   held to `MIN_SATZ`, and the search never continues past a candidate that exists but
   does not run. `SatzError::TooOld` is the banner naming the version found and
   `satz self-update`.
3. `EstateSession::open(bin, dir, main, allow)` resolves `main` as satz resolves a name
   on the command line, makes it absolute, and spawns `satz mcp --root <root> --allow
   <allow>` through `McpSession::open`. `<root>` is `session_root`: the longest common
   prefix of the estate's directory, every directory its resolved config names
   (`yaml_dir`, `hcl_dir`, `schema_dir`, `presets_dir`, each `include_dirs` entry) and
   the main file's directory — the directory itself for an estate whose config stays
   inside it, the repository root for `tests/fixtures/smoke`, whose paths reach into
   `vendor/satz`. `allow` is `Settings.mcp_allow`, `read,write` by default.
4. `McpSession::open` initializes and keeps the server's `instructions`, lists the
   tools as `ToolInfo` with their `ToolAnnotations`, reads `satz://guide`, and calls
   `satz_open {config, estate}` for the `OpenReport` with `runs_as` and
   `deployment_mode`. The child's stderr is a broadcast channel: `stderr()` subscribes
   from now on, `stderr_backlog()` returns the last 256 lines, `pid()` names the child.
   A child that has exited is `SatzError::Closed` on the next call; an initialize that
   fails is `SatzError::Mcp` carrying what satz said before it died.
5. The estate coroutine's `Reload` builds the model: `satz_questions` over the session
   for the `QuestionsReport`; then, on a blocking thread, the main file read,
   `Cst::parse`, `EstateDir::params` and `ResourceRegistry::load_all(schema_dir)`; then
   `EstateModel::build(main, cst, schema, env, questions, diagnostics)`, where `schema`
   is `Result<&ResourceRegistry, &Path>` and the `Err` arm becomes
   `SchemaStatus::Missing`. The same build runs after every commit and on the top bar's
   Reload; no file watcher runs.

The model is derived, never guessed: a key the schema does not know is `Unknown` and
read-only. `ResourceNode.missing_required` lists the schema's required attributes and
blocks the node lacks, minus what satz derives from the position. An `AttrRow` is
locked when it is `import-id`, computed-only, or inside an `Unknown` block; a value
carrying an interpolation, a reference or an object is edited in `EditMode::Source`. A
`ParamRow` joins its question; a param that gates a pack line, and every option of a
`oneof`, is a pack row instead. A `PackRow` is `PackRowKind::Map`
(the `presets/estate-map.satz` line, `Choice::Line`) or `PackRowKind::Choice` with
`Choice::Bool { current, default }` or `Choice::OneofOption { group, selected }`; its
`gate` and `question` are optional; its `state` is `On`, `Off`, or `Absent` when the
map asks a question the file has no line for, whose remedy is `satz merge-presets`. A
line active while its gate is false is a `Severity::Note` from `DiagSource::Model`.

### 4b. The write discipline

Every writer takes `EstateSession::write_lock` first and holds it across the check and
the rename: a view edit, an interview answer, an agent's write tool. The views that
call this are U8; the mechanism is complete and tested.

A value edit, `Edit::ReplaceValue { node, value }` or `Edit::ReplaceParam { name,
value }`:

1. `EditSession::open(path)` refuses a file that is not `.satz`, makes the path
   absolute, and records the bytes, their sha256 and the `Cst`.
2. `EditSession::apply(edits)` renders each value with `render_value` in the
   `style_of` its node and splices it over the node's span only; a param the block does
   not bind is appended before its `}` as satz's `bind` appends it. The proof is in two
   parts: `satz_core::satz::parse` must accept the new text (`EditError::Syntax`), and
   the new tree must equal the old one in a walk of node signatures where each edited
   value stands as a hole and each appended param as one entry; any other difference
   is `EditError::ChangedElsewhere` naming the line. Two edits on one node, one inside
   another, and two appends of one name are refused.
3. `Proposed::commit(&dyn Checker)`: a sha256 on disk that is not the session's is
   `Rollback::ChangedOnDisk`, no merge; the text is written to `<stem>.studio-tmp.satz`
   beside the file, so `use` and `include_dirs` resolve identically and the name keeps
   the extension satz's CLI reads (it refuses any other); `Checker::check(tmp)` runs;
   a refusal deletes the temp file and is `Rollback::Check` with every diagnostic
   re-pointed from the temp path to the real one (`Diagnostic::repoint`); a checker
   that could not run deletes it and is `CommitError::Satz`; the temp file is renamed
   over the real one, which is read again and hashed; `Committed { path, sha256,
   summary }` is the next session's snapshot.

`McpChecker` calls `satz_transpile_check {estate}` over the session and types its
`CompileSummary`; `CliChecker` runs `satz --config <dir> transpile <path> --check`.
Both read satz's findings as data rather than the sentences it renders them to: over
MCP a refusal's `structuredContent` carries them and a pass carries them in the
summary; on the CLI they are the `Debug` of the `CompileRefusal` the process exits on,
decoded from the final `Error:` line — one diagnostic per finding, at the file and
line it names, with its `kind`. A structured payload of a shape the app does not read
is `CheckFailure::Failed`, never a guess. A refusal that never reached the compile
carries no findings, and its text is read as satz's output; a front-end failure is the
`PipelineError` form, one located diagnostic. `CliChecker` returns an empty address
list and no findings, since the CLI prints neither as data. A test holds both to the
same verdict and the same `(file, line, kind, message)` set.

What a check that passed reported reaches the drawer too: after a write lands — a
value edit, an answer, the map line — `Committed.summary.findings` becomes diagnostics
at their lines, so a warning satz raised is visible beside the change that raised it.

A delegated write (an answer, a pack toggle, a `oneof` choice) is `satz_interview`,
satz's own writer on the real file. `Snapshot::take(path)` records the bytes first;
`Snapshot::verify(&dyn Checker)` then checks the real path and is `Committed` on a
pass; a refusal, or a checker that could not run, writes the recorded bytes back.
`EstateDir::estates` never lists a checked temp file; `.gitignore` carries the suffix.

### 4c. An agent turn

Two engines serve the Chat view and raise the same `AgentEvent`s, so the transcript
list, the tool cards and the approval card are one implementation. `Settings.provider`
chooses: **the API engine** is `Agent::run_turn(user_text, events, cancel)` over a
`ToolHost` — `EstateSession` implements the trait, and its `call` takes the write lock
for a tool that is not read-only — and **the Claude Code engine** is the installed CLI
driven over stdio on the user's claude.ai subscription
([ADR 0010](adr/0010-claude-code-as-the-subscription-backend.md), below). The rest of
this section is the API engine.

- **Request.** `Agent::request` builds `system[0]` from `PREAMBLE`, the session's
  `instructions` and the text of `satz://guide`, with the cache breakpoint, and
  `system[1]` from `EstateContext::render()`, the volatile estate context (path,
  `runs_as`, `deployment_mode`, the questions summary, up to `MAX_DIAGNOSTICS`
  diagnostics, the outline) with no breakpoint; `tool_defs` turns every `ToolInfo` into
  a `ToolDef`, `input_schema` verbatim, the output schema's keys appended as
  `Returns: {…}`. `body()` in `types.rs` owns the four breakpoints (the last tool, tools
  sorted by name; `system[0]`; the last block of the last user message; a marker set
  anywhere else is dropped) and sends adaptive thinking with a summarised display,
  `output_config.effort`, and `fallbacks: "default"` when asked
  ([ADR 0009](adr/0009-refusal-fallbacks-are-on-by-default.md)).
- **Stream.** `ClaudeClient` posts to `{base_url}/v1/messages` with `x-api-key` or a
  bearer token and the betas a request needs in one `anthropic-beta` header.
  `SseDecoder` and `Assembler` fold the events into a `Response`; a tool's input is
  parsed at `content_block_stop`, and every complete block arrives as
  `StreamEvent::BlockStop { index, block }`. A request is retried at most twice, on a
  rate limit, an overload, a server error or a connection failure, and only before the
  first byte of its stream. `ContentBlock` types the five block kinds the code reads;
  every other block is `ContentBlock::Other`, carried and replayed verbatim.
- **Tool calls.** On `StopReason::ToolUse` every `ToolUse` block runs in order. The
  gate is `runs_without_asking`
  ([ADR 0005](adr/0005-tool-approval-by-mcp-annotation-and-the-capability-ceiling.md)):
  a read-only tool runs; a non-destructive tool runs when `auto_approve_writes` is set
  or the operator allowed it for the session; a destructive tool asks every time.
  Anything else raises `AgentEvent::ToolCallPending` and waits for `Approval::Once`,
  `ForSession` or `Deny`. `Deny`, a tool the host does not list, and an input that is
  not a JSON object are each a `tool_result` with `is_error`, never a protocol error;
  all results of one assistant message go back in one user message.
- **Ends.** `EndTurn`, `MaxTokens` and `StopSequence` are `AgentEvent::TurnDone`;
  `PauseTurn` loops again, ten times at most; `Refusal` is `ClaudeError::Refused` with
  the category, the explanation and the recommended model from `stop_details`. A turn
  that is refused, cancelled or fails is rolled back whole: `messages` is truncated to
  where it began. A tool call in flight completes on its own task; its result is dropped.
- **Credential.** `Credential::resolve` tries `ANTHROPIC_API_KEY`, then
  `ANTHROPIC_AUTH_TOKEN`, then `ant auth print-credentials --access-token` when `ant`
  is on `PATH`, then the keychain entry Settings writes; `ClaudeError::NoCredential {
  tried }` says what every source answered. Settings shows which one won.
- **Other providers** implement `ChatProvider`: `OpenAiCompat`
  (`{base_url}/chat/completions`) and `Ollama` (`{base_url}/api/chat`, NDJSON) drop
  thinking, effort and cache breakpoints and say so through `Capabilities`.
- **Transcripts.** `TranscriptStore` writes `<data dir>/satz-studio/transcripts/<sha256
  of the estate path>/<created>.jsonl`: line one the `TranscriptHeader` (estate path,
  model, instant), then one `Message` per line, appended and flushed; an existing file
  is never overwritten, and a line that is neither fails the load.

### 4d. A Claude Code turn

`llm::claude_code::Session` is one `claude -p --input-format stream-json
--output-format stream-json` per open estate, spawned in the estate directory. It runs
on the claude.ai account the CLI is signed in to; the app reads no credential of Claude
Code's and asks it nothing but `claude auth status --json`.

- **The command line** (`command_args`) is `--setting-sources ""` and
  `--strict-mcp-config` so the user's own Claude Code settings and MCP servers stay out,
  `--tools ""` so no built-in tool is available, `--mcp-config` naming one server —
  this estate's `satz mcp --root <session root> --allow <ceiling>` — `--permission-mode
  default`, `--permission-prompt-tool stdio`, `--allowedTools` from the annotations satz
  declares ([ADR 0005](adr/0005-tool-approval-by-mcp-annotation-and-the-capability-ceiling.md)),
  and `--append-system-prompt` with `PREAMBLE`, the MCP server's instructions, the text
  of `satz://guide` and which estate this session is about. `--bare` is never passed:
  bare mode does not read the subscription login.
- **The protocol.** The client writes one JSON object per line on stdin: an initialize
  control request at spawn, then `{"type":"user",…}` per turn, a `control_response` per
  approval, and an `interrupt` control request to cancel. The CLI writes one per line
  on stdout: `system/init` (the session id, the model, the MCP servers' status — read
  at the first turn, not at spawn), `stream_event` carrying the Messages API's own
  events, `user` messages holding the results of the tools it ran, `rate_limit_event`,
  `can_use_tool` control requests, and a `result` line ending the turn.
- **The translation.** `stream_event` payloads go through the same `Assembler` the API
  engine uses — a new one per `message_start`, since one turn is many assistant
  messages — and its `StreamEvent`s become `TextDelta`, `ThinkingDelta`,
  `ToolUseStarted` and `ToolInputDelta` under the name satz gives the tool, with the
  `mcp__satz__` prefix stripped. A `tool_result` block becomes `ToolResult`, a
  `can_use_tool` request becomes `ToolCallPending` whose answer is the control response
  (`ForSession` remembers the tool, so the next call needs no card), `rate_limit_event`
  becomes `AgentEvent::Notice` for the footer, and `result` becomes `TurnDone` —
  `EndTurn`, or `MaxTokens` for `error_max_turns`.
- **The estate's write lock is held for the whole turn**, not per call: Claude Code's
  satz server writes the estate, and the app cannot see the calls it pre-approved. The
  app keeps no transcript for this engine — Claude Code holds the conversation, "New"
  starts a fresh process, and the model comes from Settings because it is an argument of
  that process.

## 5. Deployment and CI

`Dioxus.toml` names the bundle identity; `dx bundle --release --platform desktop`
produces the bundle per OS, and U10 builds the release workflow that runs it. The
webview is a runtime dependency: WebView2 on Windows, `webkit2gtk-4.1` on Linux.

`.github/workflows/ci.yml` runs `core` on `ubuntu-24.04` for every push and pull
request: the desktop crate's system libraries, then `scripts/install-satz.sh`, then
`cargo fmt -p satz-studio-core -p satz-studio -- --check` (the two packages;
`vendor/satz` is satz's own), `cargo clippy --workspace --all-targets --locked -- -D
warnings`, `cargo test --workspace --locked` and `cargo build -p satz-studio --locked`.
`install-satz.sh` downloads the cargo-dist installer of the newest satz release with
its SHA-256 sidecar, refuses an installer without one, and refuses a release below
`MIN_SATZ`; a tag given as its one argument installs that release instead. It follows
the newest because satz keeps only its five newest releases, so an installer asset
pinned by tag is gone within days, and the app's contract is `MIN_SATZ` or newer. It
also writes the runner's satz config (`self_update_frequency = "never"`) when none
exists, so no update check runs while the tests read `--format json`. `platforms` (`macos-15`,
`windows-2022`) runs the same formatting, clippy, test and build steps on every push
and pull request; on Windows satz is built from the submodule, since satz has no
Windows release. `.github/workflows/names-gate.yml` runs `scripts/check-names.sh` over
the tree and over the commits each push or pull request adds.

## 6. Decisions

| record | decision |
|---|---|
| [0001](adr/0001-dioxus-desktop-on-the-webview-renderer.md) | Dioxus 0.7 desktop on the webview renderer |
| [0002](adr/0002-a-separate-repository-with-satz-pinned-once.md) | a separate repository; satz pinned once, as the submodule; the binary required at `MIN_SATZ` |
| [0003](adr/0003-the-document-layer-is-the-tree-sitter-grammar.md) | the document layer is the tree-sitter grammar, vendored and compiled in; satz-core stays the authority on meaning |
| [0004](adr/0004-claude-natively-other-providers-adapt-into-its-message-model.md) | Claude natively: the Messages API wire types are the app's message model; other providers adapt into it |
| [0005](adr/0005-tool-approval-by-mcp-annotation-and-the-capability-ceiling.md) | tool approval by the MCP annotations satz declares; the capability ceiling stays satz's |
| [0006](adr/0006-apply-and-bootstrap-run-in-the-users-terminal.md) | `apply` and `bootstrap` run in the user's terminal, never with `-auto-approve` |
| [0007](adr/0007-pack-rows-are-derived-from-the-estate-file.md) | pack rows are derived from the estate file, the questions report and the resolved params; no copied table |
| [0008](adr/0008-transcripts-live-outside-the-estate.md) | transcripts live under the app's data directory, never inside an estate |
| [0009](adr/0009-refusal-fallbacks-are-on-by-default.md) | refusal fallbacks are on by default, off by a Settings switch |
| [0010](adr/0010-claude-code-as-the-subscription-backend.md) | Claude Code as the subscription backend: the installed CLI driven over stdio, the estate's satz MCP server, the app's own approval card |

## 7. Not built, and why

- **No embedded terminal.** `apply` and `bootstrap` hand stdio to tofu and to the
  human; tofu's approval prompt is the safety step. `EstateSession::external_command`
  writes a one-shot script under `<data dir>/satz-studio/run/` holding `cd "<estate>"
  && "<satz>" --config . <args…>`, and `open_in_terminal` opens it with the OS
  terminal. The app learns nothing from the terminal; `-auto-approve` is never passed.
- **No copy of satz's `PACK_LINES` table.** A pack row is derived from three sources
  the estate already has: `scan_uses` over the file, the questions report, and the
  resolved params. A gate with no line is `LineState::Absent`, and the row's action is
  `satz merge-presets`, which is what satz tells an operator.
- **No second parser.** The document layer is the tree-sitter grammar and meaning is
  `satz_core::satz::parse`. A grammar gap is fixed in the grammar repository.
- **No markdown renderer in the chat.** The model's text is shown as text; the tool
  calls, their results and the approval cards are the structure.
- **No merge.** A file that changed on disk under an edit is refused and reloaded.
