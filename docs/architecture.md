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

- **satz owns the estate.** The writers for answers (`satz_interview`), for pack
  switches (`satz_add_pack`, `satz_remove_pack`) and for import ids (`satz adopt`) are
  satz's; the app calls them and
  never re-implements them. What the app writes itself is one value inside its span,
  checked by `satz transpile --check` before it replaces the file.
- **The app never touches `hcl_dir`.** The generated HCL is satz's output; the app reads
  the estate's `config.toml` to learn where it is and leaves it alone.
- **One identity per estate.** Every open estate has its own `satz mcp` child; the
  identity its live tools run as is what `satz_open` returned (`runs_as`), shown in the
  Overview's identity card and configured nowhere.
- **Fail fast, no degraded modes.** A satz older than `MIN_SATZ` is refused at startup;
  a settings file that does not parse is a full-screen refusal; a `schema_dir` without
  a schema is `SchemaStatus::Missing`; a file that changed on disk under an edit is
  refused, never merged; a JSON field satz removed fails the deserialisation.
- **A refusal offers what can be done about it.** `SatzError::TooOld` carries the PATH of
  the binary it refused, so the banner and Settings can run `satz self-update` on that very
  binary — a too-old satz updating itself is the only way out of the refusal without
  leaving the window. satz owns its updater (it checks GitHub, verifies the sha256 sidecar
  and runs the installer), so the app runs the command and shows what it said rather than
  fetching anything; it passes `--no-open-readme`, because a successful update otherwise
  opens the documentation site in a browser, which is right on a terminal and wrong under a
  window. The operator's own `self_update_frequency` is never written. On Windows
  `satz self-update` checks but refuses to install, so there the update is satz's
  PowerShell installer, run by the app into the folder of that binary. With no satz at all,
  the banner and Settings run satz's own installer (`satz::install`, below). The app does
  not update ITSELF: that waits for code signing. It looks for a newer satz-studio release
  and offers its page.
- **The app copes with a newer satz and tells the operator**
  ([ADR 0014](adr/0014-a-newer-satz-is-a-notice-and-the-app-looks-for-releases.md)).
  `MIN_SATZ` is the oldest satz this build works with, and a satz below it is the one
  run-time refusal. `SatzBinary::built_against()` is the satz the build is built and tested
  against — the version of the submodule, read from its manifest as the crate compiles —
  and `ahead_of_build()` says whether a located satz is past it by a patch or by a minor,
  satz's own reading of a release (satz ADR 0010, `vendor/satz/docs/adr/`): a minor is one
  after which an estate may need edits, be refused, or plan differently. A newer satz is
  `SatzStatus::Located` and every estate opens; the banner states the gap until the
  operator dismisses it for that version (`Settings.dismissed_satz`). Nothing refuses a
  newer satz, because CI installs the NEWEST satz release on purpose: on the day after a
  satz release every test that locates the real binary sees a newer satz, and a red run
  then is the alarm answered with a satz-studio patch release.
- **`MIN_SATZ` rises for a reason, not with the pin.** It may sit below the submodule's
  satz; `min_satz_is_not_newer_than_the_submodule` holds it at or below. It rises only
  when (1) a satz release breaks the app, which a satz-studio patch release answers the
  same day, or (2) the app starts using something a later satz introduced — a flag, a
  tool, a report field — whose tests are what make that version the requirement. A routine
  pin bump moves the submodule and the recorded reports and leaves `MIN_SATZ` alone:
  raising it with every bump would refuse a satz the app still works with and send the
  operator to update for nothing. The accepted cost is that the minimum is not run against
  itself: satz keeps only its five newest releases, so CI cannot install the minimum to test
  it, and the minimum rests on (1) and (2) rather than on a run against that exact
  version.
- **The app looks for releases on its own, once per launch** — never on a timer, the
  result kept for the session. It reads the latest satz-studio release from GitHub
  (`github::look_for_studio_update`) and runs `satz self-update --check-only` on the satz
  in use, because satz owns its updater. The satz check is skipped when the operator's satz
  config (`~/.config/satz/satz.toml`) says `self_update_frequency = "never"`: satz may not
  look unprompted, and the app does not look on its behalf. The window title carries the
  app's version and "update available" for either release; Settings is where either is
  acted on — the release page for satz-studio, `satz self-update` for satz — and the
  banner offers the satz-studio release while it names a newer satz. A look that fails says why in Settings and raises no toast.
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
| `src/estate.rs` | an estate directory as satz sees it: `config.toml` read into `ToolConfig` with satz's defaults and resolved against its own directory; `discover` walks a folder for every `config.toml` (depth 6, at most 200, skipping `hcl/`, `target/`, `evidence/`, `node_modules/` and dot-directories); `estates` lists the `.satz` files in `yaml_dir` that declare an `estate`, skipping a checked temp file (`is_checked_temp`); `loader` resolves `use "…"` as satz does (the file's directory, then `include_dirs`); `params` and `deployment_mode` read the resolved params without a schema, and `acknowledged` asks satz's own rule whether those params acknowledge a pack's notice; `HclState::read` answers two facts about `hcl_dir` and no more — `main.tf` is there, so the estate has been transpiled here, and `.terraform` is there, so the tool's init has run | `EstateDir`, `ToolConfig`, `EstateError`, `HclState`, `declares_an_estate` |
| `src/cst/` | the lossless document layer over the vendored tree-sitter grammar ([ADR 0003](adr/0003-the-document-layer-is-the-tree-sitter-grammar.md)); `grammar.rs` exposes the compiled parser, `build.rs` walks the tree into nodes with byte spans, `uses.rs` and `render.rs` read the pack lines and write values | `Cst`, `Node`, `NodeKind`, `Span`, `UseLine`, `UseState`, `TypedValue`, `StyleCtx`, `scan_uses`, `render_value`, `style_of`, `grammar::language` |
| `src/edit/` | the edit primitives and the write discipline (section 4b): `apply.rs` the splice and its proof, `commit.rs` the temp file and the rename, `check.rs` the two checkers, `snapshot.rs` the delegated write | `Edit`, `EditSession`, `Proposed`, `Committed`, `Rollback`, `Checker`, `CheckFailure`, `McpChecker`, `CliChecker`, `Snapshot`, `sha256_hex` |
| `src/schema.rs` | the provider schema as `satz update-schema` writes it, the types lifted from satz's `src/schema.rs`; `load_all` reads every `*.json` in `schema_dir` and is `SchemaError::Missing` for a directory that is absent or holds no resource type; `AttrType` decodes Terraform's type expression and prints it in Terraform's spelling | `ResourceRegistry`, `AttrType`, `BlockSchema`, `AttributeSchema`, `SchemaError` |
| `src/model/` | the view model, built pure and rebuilt after every commit and reload: `outline.rs` classifies the blocks as satz's `EstateResolver` and `is_child` do, `params.rs` joins the `params { }` block with the questions and holds `answer_kind`, the shape an interview answer is typed in — satz's own, read off the report, `value.rs` decodes a string as satz's lexer reads it, and `hcl_blocks` reads the file's `hcl` statements with whether each carries a `trust` reason. The packs are satz's `PacksReport`, carried as `satz_packs` returned it, with the phase comment above each `use` line by its number ([ADR 0018](adr/0018-the-packs-view-shows-satzs-pack-graph.md)) | `EstateModel`, `ResourceNode`, `ResourceKind`, `AttrRow`, `ParamRow`, `ParamKind`, `SourceValue`, `StrPart`, `EditMode`, `HclBlock`, `SchemaStatus`, `answer_kind` |
| `src/git.rs` | what `satz merge-presets` needs from git: it edits the estate file in place and asks `git status` in the estate file's directory for the undo, refusing outside a work tree or without git. `WorkTree::read` asks `git rev-parse --is-inside-work-tree` in that directory — a repository above it counts — and answers `Inside`, `Outside` with git's own words, or `NoGit`; `init_steps` are `git init -b main`, `git add -A` and one commit naming the estate; `run` streams one git command's lines and is cancellable | `WorkTree`, `GitError`, `init_steps`, `run` |
| `src/satz/binary.rs` | where satz is and which version: the Settings override, `PATH`, `~/.local/bin/satz` (`satz.exe` on Windows — `home_bin_dir` and `in_dir`, the folder the installer writes too); the gate that refuses a satz older than `MIN_SATZ`, the oldest satz this build works with; `built_against` is the submodule's satz version, and `ahead_of_build` reads a satz past it as a patch or a minor ahead and refuses nothing | `SatzBinary`, `MIN_SATZ`, `Ahead` |
| `src/satz/self_update.rs` | what `satz self-update --check-only` printed, read narrowly: the `Latest version:` line against the version of the satz asked, and the `Release:` line when there is one — an output without the line is an error quoting it; `unprompted_checks_allowed` reads `self_update_frequency` from the operator's `~/.config/satz/satz.toml` as satz reads it, a missing file or key being `always` and a file that does not parse an error | `SatzRelease`, `read_check`, `unprompted_checks_allowed` |
| `src/satz/install.rs` | satz's own cargo-dist installer, for an operator with no satz and, on Windows, for an update: `Installer` names the two — `satz-installer.sh` under `sh`, `satz-installer.ps1` under `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File` — with their `.sha256` sidecars, and `for_this_system` picks one. The installer and its sidecar are the assets of ONE `releases/latest` object, so a release published between two downloads cannot pair them; `VerifiedInstaller::verify` is the only way to hold the script, and only on a matching SHA-256; `run` writes it under its asset name into a private temporary directory and runs it with `SATZ_INSTALL_DIR` naming the folder the caller gives, `SATZ_NO_MODIFY_PATH=1` (the installer otherwise adds its folder to `PATH` in the shell profiles or the Windows user `PATH`, and `locate` searches `~/.local/bin` without it) and stdin closed, streamed and cancellable | `Installer`, `VerifiedInstaller`, `InstallError`, `fetch_verified` |
| `src/satz/export.rs` | the decisions sheet and the workbook: `formats` reads the values of `--format` from the installed satz's `satz questions --help` — the `Possible values:` block, each with satz's own line — and a help without one is an error, never an empty list; `args` is `questions <estate> --format <format> --out <file>` and nothing else; `destination` gives a chosen path the format's extension when it has none, so the file the app opens is the file satz wrote; `written` refuses a file that is missing or empty after a clean exit | `QuestionsFormat`, `formats`, `parse_formats`, `args`, `destination`, `written` |
| `src/satz/cli.rs` | `satz --config <dir> <args…>` in the estate's directory, stdout and stderr streamed line by line and cancellable, by the one streaming helper the installer's run shares; `help` reads a command's long help; `json_report` runs a reporting command with `--format json` and an `--out` of its own and types the file it wrote; `json_verdict` does the same for a command whose exit status is a verdict on what it judged (`review-pack`), returning the report with the status, and a non-zero exit that wrote no file is an error; `run_in` is the same streaming without a `--config`, in a working directory of its own, for the one command that runs before a `config.toml` exists | `SatzCli`, `CliLine` |
| `src/satz/init.rs` | `satz init` as a typed thing: `InitOptions` renders the flags it was given to argv and passes nothing for a field left blank, so a blank field is the instruction to derive; `check_target` refuses a directory that is not there or already holds a `config.toml`; `created` reads what a finished run left, because `init` names the estate file after a customer id it may have derived and the name is not knowable in advance | `InitOptions`, `check_target`, `created` |
| `src/satz/mcp.rs` | one `satz mcp` child per estate, spoken to with rmcp over stdio; every rmcp type stays inside this file. A tool's refusal is a `ToolOutcome` with `is_error`; a JSON-RPC `invalid_params` error in place of a result — a tool name satz does not serve — is `SatzError::InvalidParams` naming the tool | `McpSession`, `ToolInfo`, `ToolAnnotations`, `ToolOutcome` |
| `src/satz/session.rs` | one session per open estate: the CLI runner, the MCP child, the write lock every writer takes, the identity from `satz_open`; `apply` and `bootstrap` as a one-shot script in the OS terminal | `EstateSession`, `session_root` |
| `src/satz/reports.rs` | serde mirrors of what a reporting command writes with `--format json` and satz returns as `structuredContent`: unknown fields ignored, missing required fields fail; the questions report round-trips a recorded output of the pinned satz. `Finding` is satz's own list of what the compile found after the front end — a `CompileSummary` carries the warnings and infos it did not refuse on, a `Refusal` the ones it did; `kind` is the kebab-case word satz writes, kept as a `String` so a kind satz adds is carried instead of failing the result. `NoticeRow` is what a pack asks to be run once it is on, with the param that acknowledges it; `severity` is required and typed — `error` is the one that holds up every command writing to the organisation — so a notice without one, or with a word satz adds, fails the report rather than reading as one that holds nothing up, and an interview report without `notices` fails too. `PacksReport` is `satz_packs`: one `PackRow` per node of satz's pack graph — its role, gate, answer, default and value, where its `use` line stands (`PackLine`, at its line number), whether it deploys, what it `requires` (each a `Requirement` with `met`), what it is `required_by` and `excludes`, its notices and the compile's findings about it — the `use` lines the graph does not know, and the findings with the pack as `subject` and the command that answers each as `fix`. Every field satz always sends is required, and a line state satz adds fails the report; it round-trips a recorded `satz packs --format json` of the smoke estate. `AddPackArgs`, `RemovePackArgs` and `PackChange` are the arguments and the result of `satz_add_pack` and `satz_remove_pack`. `MergeReport` is `satz_merge_presets`: its events (`MergeEvent`, tagged by `kind`; an event kind satz adds fails the report, an outcome word satz adds is carried), its `MergeCounts`, `attention` and the notices it opened; `lines()` is the report as the command log shows it, in sentences; it round-trips a recorded merge of the smoke estate. `PackReview` is `satz review-pack --format json`: the pack, the estate it was folded into, what it emits and its findings, every field required; `passed()` is satz's verdict — no finding is an error; it round-trips two recorded reviews of satz 0.73.1, one clean and one broken | `QuestionsReport`, `QuestionRow`, `InterviewArgs`, `InterviewReport`, `NoticeRow`, `PrerequisitesResult`, `OpenReport`, `CompileSummary`, `Finding`, `FindingSeverity`, `Refusal`, `PacksReport`, `PackRow`, `PackRole`, `PackLine`, `Requirement`, `RequirementKind`, `Unmanaged`, `AddPackArgs`, `RemovePackArgs`, `PackChange`, `MergeReport`, `MergeEvent`, `MergeCounts`, `PackReview` |
| `src/satz/review.rs` | `satz review-pack` and the two places a reviewed pack goes ([ADR 0019](adr/0019-the-pack-review-runs-the-cli-and-places-a-private-pack-as-a-local-fork.md)): `review` runs `satz --config <estate dir> review-pack <pack> [--against <estate>] --format json` through `json_verdict`, holds the exit status to the report's own verdict (`SatzError::Verdict` when they disagree) and keeps the bytes it judged, refusing a pack that changed while satz read it; `diagnostics` is each finding at its `file:line` from `satz review-pack`. `local_name` is `<stem>.local.satz` — a `.local.satz` keeps its name, a `.diff.satz` is refused — and `upstream_name` is `presets/<stem>.satz`. `place_private` writes the reviewed bytes into `presets_dir` under that name: refused when the pack changed since its review or the library is missing, nothing written when the file holds these bytes already, refused when it holds anything else; the file is created with `create_new`, the estate is checked with it in the library, and a refusal or a checker that could not run removes it again | `ReviewedPack`, `review`, `review_args`, `diagnostics`, `local_name`, `local_target`, `upstream_name`, `place_private`, `Placed`, `PlaceError` |
| `src/satz/mod.rs` | the capability ceiling and the one error type of the driver | `Allow`, `SatzError` |
| `src/llm/claude/` | Claude natively ([ADR 0004](adr/0004-claude-natively-other-providers-adapt-into-its-message-model.md)): `types.rs` the Messages API wire types as the app's only message model and `body()`, `sse.rs` the event-stream decoder and the assembler, `client.rs` the HTTPS client, `error.rs` the one error type | `Request`, `Response`, `Message`, `ContentBlock`, `SystemBlock`, `ToolDef`, `StopReason`, `StopDetails`, `Usage`, `Effort`, `ClaudeClient`, `ClaudeError` |
| `src/llm/agent/` | the agent loop over a `ToolHost`, the approval gate, and `bridge.rs`: MCP tools as Claude tool definitions and outcomes back as `tool_result` blocks | `Agent`, `AgentEvent`, `Approval`, `ToolHost`, `EstateContext`, `tool_defs`, `tool_result` |
| `src/llm/auth.rs` | where a credential comes from; the keychain entry `satz-studio` / `anthropic-api-key` | `Credential`, `CredentialSource` |
| `src/llm/provider/` | the providers that are not Claude, mapping the Claude-shaped request into their wire format and their stream back; `Capabilities` says what each drops | `ChatProvider`, `StreamEvent`, `Capabilities`, `OpenAiCompat`, `Ollama` |
| `src/llm/claude_code/` | the Claude Code engine ([ADR 0010](adr/0010-claude-code-as-the-subscription-backend.md)): `cli.rs` where the binary is, its version and `claude auth status`; `events.rs` the lines the CLI writes, typed; `session.rs` one process per estate, its command line, the turn, the approval round trip and the interrupt; `log.rs` the stream log ([ADR 0013](adr/0013-the-claude-code-stream-log-is-verbatim-off-by-default-and-bounded.md)) | `ClaudeCodeCli`, `AuthStatus`, `ClaudeCodeError`, `CcLine`, `Session`, `SessionOptions`, `StreamLog`, `StreamLogConfig`, `Channel` |
| `src/transcript.rs` | conversations as JSONL under the app's data directory, outside the estate ([ADR 0008](adr/0008-transcripts-live-outside-the-estate.md)) | `TranscriptStore`, `Transcript`, `TranscriptHeader` |
| `src/github.rs` | the latest release of a repository through GitHub's unauthenticated REST API, always `releases/latest` and never a tag; `look_for_studio_update` compares satz-studio's with the running version and downloads nothing; a 403 or 429 from the API is `RateLimited` with the reset, a connection that fails is `Unreachable`, a 404 is `NoRelease` | `Release`, `Asset`, `GithubError`, `StudioUpdate`, `latest_release`, `download`, `look_for_studio_update` |
| `src/settings.rs` | `<config dir>/satz-studio/settings.toml`: a missing file is the first run, a broken one is an error; no credential in it; `dismissed_satz` is the satz release newer than the build whose notice the operator dismissed, and it permits and refuses nothing; `data_dir` is where transcripts, the Claude Code stream logs and the one-shot scripts go | `Settings`, `ProviderChoice`, `Theme`, `settings_path`, `data_dir` |
| `src/diag.rs` | the one diagnostic type: `Diagnostic::from_finding` turns one of satz's findings into it — the severity mapped, the `kind` carried, a relative file resolved against the estate's directory, the group's header in front of the message — and `parse_satz_output` reads what satz prints: its findings in the layout it gives a reader (a group's title, per finding a row of severity, kind, `file:line` and subject with the message and `fix:` indented under it, a block of rows sharing one message, the footer of counts) as one diagnostic per row with the same message `from_finding` builds, and around them `file:line: msg`, `satz: line N: msg`, the severity prefixes, the banner dropped, an indented line continuing the one above | `Diagnostic`, `Severity`, `DiagSource`, `parse_satz_output` |

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
  `Discover`, `OpenEstate`, `CloseEstate`, `CreateEstate`, `CancelCreate`,
  `ImportEstate`, `CancelImport`, `UpdateSatz`, `CancelUpdate`, `DismissSatzNotice`,
  `LookForStudioUpdate`, `InstallSatz`, `CancelInstall`, `SaveSettings`,
  `ResolveCredential`, `StoreKey`; at startup it locates satz, makes the two release looks
  and walks `last_root`;
- **one coroutine per open estate** (`src/state/estate_actions.rs`, `EstateAction`),
  started by `EstateHost` in `src/shell/mod.rs` with the `Arc<EstateSession>` and
  living as long as the estate is open: `Reload`, `RunCommand`, `RunNoticeCommand`,
  `CancelCommand`, `OpenInTerminal`, `Answer`, `AcceptDefaults`,
  `WritePrerequisites`, `CommitEdit`, `AddPack`, `RemovePack`, `MergePresets`,
  `ReviewPack`, `PlacePrivate`, `CloseReview`, `InitRepository`, `Close`, `Switch`.

A reporting command takes one `--format` and one `--out`, both required, and writes one
file instead of printing (satz's ADR 0021). The app names the destination: the file the
command's own `--out` field names, which is the estate's and stays, else one under
`reports_dir()` — the app's directory in the system temporary directory — which
`RunCommand` reads into the log after the streamed lines and removes. `update-prerequisites`
is the one command in the palette the ADR leaves on the console, and the palette runs it
with `--report-only`, in satz's default text: the command writes the estate file by default, and a write from
the palette would hold no write lock and reload no model. The writing run is offered in
Checks instead, as `EstateAction::WritePrerequisites` — `satz_update_prerequisites
{report_only: false}` through the delegated-write discipline of section 4b.

A pack can name one command to be run once it is switched on — its `notice`, which the
estate acknowledges by binding the notice's param `true` (satz's ADR 0033). satz returns
the notices a write opened in that write's own report, once, and the app holds them:
`queued()` puts what `satz_interview`, `satz_add_pack` and `satz_merge_presets` returned into
`EstateStore::notices` and raises the dialog of `src/shell/notices.rs`. What takes one
away is the estate binding its param, and satz decides that — every reload asks
`EstateDir::acknowledged` over the params it has just folded — so "I ran it" (an `Answer`
binding the param) and `satz adopt --execute --import` (which binds it itself) both end
the same way. `RunNoticeCommand` is the notice's own command run in the app: the same
run as `RunCommand`, followed by a reload, because that command writes the estate file.

`apply`, `bootstrap` and `migrate` run in the user's own terminal
([ADR 0006](adr/0006-apply-and-bootstrap-run-in-the-users-terminal.md),
[ADR 0012](adr/0012-migrate-hands-off-to-the-terminal.md)); `bootstrap --dry-run` is a
separate palette entry that creates nothing and runs in the app, which is what the
Overview's day-0 row offers.

The shell (`src/shell/`) is the navigation rail with its badges, the top bar with the
`runs_as`, satz-studio-version and satz-version chips and the actions beside them, the
`SatzBanner` while satz is missing, too old or does not run, and as a notice while it is
newer than the build, the diagnostics drawer, the commands palette, the notice
dialog and the snackbar host.

The Start screen is the way in, and the only one: a row of doors (`state::Door`) over
the pane the chosen door opens. **Create** (`src/views/create.rs`) is the `satz init`
form and its run log; **Import** (`src/views/import.rs`) is the `satz import` form, its
plan and satz's report; **Open** (`src/views/estates.rs`) is the folder walk and its
estate cards. A door is one `Door` variant, one card in the row and one arm of the
view's `match`, so another way in joins by being added in those three places.

With an estate open the window is ordered by the job: **Overview**, **Packs**,
**Decisions**, **Estate**, **Checks**, **Deploy**, then Chat and Settings at the foot of
the rail — Packs before Decisions, because a pack is what declares a question. Overview
(`src/views/overview.rs`) says which estate this is — the short name, the customer, the
customer id and the organisation id from the file's own `params { }` block (the model's
`params`), then the answers its other `estate_core` questions carry (`identity()` over
both), and derives what
it still has to do from its own state on every render — `owed()` over `Facts`, pure and unit-tested — and shows nothing
when the list is empty; nothing about how the estate reached the app is remembered, so
created, imported and opened estates show the same list. That includes the repository:
`satz init` makes none, and `satz merge-presets` refuses outside one, so whether git
holds the estate file's directory is read at every reload (`WorkTree::read`) and an
estate outside a repository owes one from its first minute, with `InitRepository` —
`git init -b main`, `git add -A`, one commit, streamed — as the button that makes it.
The app runs git only when that button is pressed, and sets no identity for the
commit. Decisions and the Overview each carry the export card (`src/views/export.rs`):
the decisions sheet or the workbook, written by `satz questions --format <format> --out
<file>` as an `Export` action of the estate coroutine, which runs it like any command
into the shared log, then checks the file is there and not empty and opens it. The
formats are read once per session from `satz questions --help`, so the picker is the
installed satz's own set; a help the app cannot read is a toast and the card's text. Commands stopped being a
destination: `PALETTE` is a table and `CommandDeck` renders any group of it, so Checks
and Deploy each gather their own and the palette over the window (⌘K) holds them all.
Packs carries the pack review (`src/views/review.rs`): `satz review-pack` over a pack
file the operator chooses, its findings in the drawer and beside the pack's text, and
the two places a reviewed pack goes — upstream by a pull request the card describes,
private as a `.local.satz` in the estate's library. The Gallery is behind `SATZ_STUDIO_DEBUG`. `docs/ui.md` is the whole map, including why
six primary destinations is the limit of the navigation-rail pattern.

## 4. Runtime views

### 4a. Creating, importing and opening an estate

An estate is created once, or imported once out of what already exists, and opened every
time after that. All three end in the same place: an `EstateSession` on one `.satz`
file.

**Create** is `satz init` in a folder that holds no estate yet. The Create form builds
an `InitOptions`, `check_target` refuses a folder that is not there or already carries a
`config.toml`, and `SatzCli::run_in(bin, dir, argv, …)` runs the command with the folder
as its working directory and **no `--config`** — `init` is what writes `config.toml`, so
there is no file for a `--config` to name, and satz refuses `--config <dir>` for a
directory without one. The lines stream into `AppStore.create` the way a command's stream into the estate's log.

`init` is live and credentialed: what the form did not state it derives from the
Application Default Credentials — the customer's domain and first administrator from the
ADC identity, the directory id and organisation id from an `organizations:search`, the
billing account from `billingAccounts.list` — and prints where each value came from.
Those values are the customer's. They are written into the estate satz creates, shown in
the run log while the window holds it, and put nowhere else: not in `Settings`, not in a
transcript, not in a file of the app's own.

When the run ends, `init::created(dir)` READS what it left — `EstateDir::open` plus
`estates()` over the folder. The estate's name is never predicted: `init` names the file
after the customer id, which it may have derived. Exactly one estate is opened as below.
None is the honest answer that `init` had no customer id, stated or derivable, and wrote
the directories and the config without an estate file; the run log carries what satz
said and the folder is left as satz left it.

**Import** is `satz import`, and it is two commands rather than one. `satz import`
imports INTO a project: run in a directory that holds no `config.toml` it refuses and
creates nothing. `import::plan(dir)` is that decision — `ImportPlan::Import` for a
directory that already holds one, `ImportPlan::InitThenImport` for one that does not —
and it is read from disk in the runner, never taken from the form, so a form filled in
before the folder changed cannot run an `init` over an estate or skip one that is needed.
The two-step path runs `satz init` first with the Terraform tool and the provider-schema
set the form carries, then the import, both through `SatzCli::run_in` and both streaming
into `AppStore.import`.

The SOURCE decides the shape and the shape decides the flags. `ImportOptions::argv`
renders `--only`, `--exclude`, `--all`, `--on-collision`, `--customer-shortname`,
`--output` and `--verbose` for a state document or a live scope, `--wrap-all` for
Terraform HCL — never a
flag of another shape. `--into` is not among them: importing only what an open estate
does not already declare grows an estate that is open, which is not what this door does.
The source is checked in two halves. `ImportOptions::check_source_exists` is the cheap
one the form asks on every keystroke: the source is there, and a live scope is one.
`ImportOptions::check_source` adds the state shape's own — satz reads what
`tofu show -json` writes and tells it from a raw `.tfstate` by `values.root_module`, and
answering that means parsing the document, which for a real organisation is megabytes. So
the runner asks it once, before a child is spawned, and the form states the requirement
under the field; the refusal is not learnt by failing a run.

What the run wrote is READ BACK. The file name differs by shape — `discovered.satz` from
a state document or a live scope, `imported-hcl.satz` from Terraform — and `--output`
moves it again, so
`ImportOptions::write_dirs` names the directories this shape writes into, `satz_files`
hashes the `.satz` files there before the import, and `written_since` answers the files
that are new or whose bytes changed. The hash rather than a listing is what makes a
second import over the same `discovered.satz` read as a file written. Exactly one written
file declaring an estate is the estate that opens; a written file that declares none is a
converted pack and opens nothing, which the outcome says; no written file at all is a
failure whatever the exit status was.

`satz import` has no `--format json`, so the report is its console output.
`ImportReport::of` splits the streamed lines — everything on stdout, plus the stderr
lines satz marks `warning:`, `error:` or `import:` — into what it wrote, what it skipped
with the reasons and levers satz names, the params it could not derive with satz's own
reason for each, its warnings, and the rest, in order. It rewrites no line and drops
none: a section that stops matching moves its line to `rest`. A live import also prints
what it read from the credentials — an organisation id, a directory id, a billing
account, an administrator's address. That is the same class of value `init` derives and
the same rule holds: into the estate satz wrote, into the run log the window shows, and
nowhere else.

**Open** is the folder walk:

1. The Start screen walks a folder with `EstateDir::discover`; each `config.toml` is
   opened and its estates listed with their `deployment_mode`, on a blocking thread.
2. `SatzBinary::locate(override)` takes the Settings path when it is set (a path that
   does not exist is `SatzError::NotFound` naming it), else `satz` on `PATH`, else
   `~/.local/bin/satz` (`satz.exe` on Windows) — the lookup that finds a satz installed
   while the app runs, whose `PATH` is the one it started with; the first candidate that exists is run with `--version` and
   held to `MIN_SATZ`, and the search never continues past a candidate that exists but
   does not run. `SatzError::TooOld` is the banner naming the version found and
   `satz self-update`; `NotFound` is `SatzStatus::Missing`, whose banner offers satz's
   installer; any other failure is `SatzStatus::Unusable`. A satz that is located is
   `SatzStatus::Located`, whatever newer release it is, and only `Located` opens an
   estate; `satz_notice` is the banner's notice for a satz past the build's.
3. `EstateSession::open(bin, dir, main, allow)` resolves `main` as satz resolves a name
   on the command line, makes it absolute, and spawns `satz mcp --root <root> --allow
   <allow>` through `McpSession::open`. `<root>` is `session_root`: the longest common
   prefix of the estate's directory, every directory its resolved config names
   (`yaml_dir`, `hcl_dir`, `schema_dir`, `presets_dir`, each `include_dirs` entry) and
   the main file's directory — the directory itself for an estate whose config stays
   inside it, the repository root for `tests/fixtures/smoke`, whose paths reach into
   `vendor/satz`. Directories that share no component have no common prefix, and the
   empty path is not an answer: `session_root` returns `SatzError::NoCommonRoot` naming
   them, because the root is the boundary `satz mcp` enforces. The session keeps the root
   it opened with, so the `--mcp-config` payload the Claude Code engine writes carries
   that same root rather than deriving a second one. `allow` is `Settings.mcp_allow`,
   `read,write` by default.
4. `McpSession::open` initializes and keeps the server's `instructions`, lists the
   tools as `ToolInfo` with their `ToolAnnotations`, reads `satz://guide`, and calls
   `satz_open {config, estate}` for the `OpenReport` with `runs_as` and
   `deployment_mode`. The child's stderr is a broadcast channel: `stderr()` subscribes
   from now on, `stderr_backlog()` returns the last 256 lines, `pid()` names the child.
   A child that has exited is `SatzError::Closed` on the next call; an initialize that
   fails is `SatzError::Mcp` carrying what satz said before it died.
5. The estate coroutine's `Reload` builds the model: `HclState::read(hcl_dir)`;
   `WorkTree::read` of the estate file's directory; `satz_questions` and `satz_packs`
   over the session for the `QuestionsReport` and the `PacksReport`; then, on a blocking
   thread, the main file read, `Cst::parse`, `EstateDir::params` and
   `ResourceRegistry::load_all(schema_dir)`; then
   `EstateModel::build(main, cst, schema, env, questions, packs, diagnostics)`,
   where `schema` is `Result<&ResourceRegistry, &Path>`
   and the `Err` arm becomes `SchemaStatus::Missing`. A reload that built a model then
   runs `satz_transpile_check` and folds the compile's own findings into the
   diagnostics — a prerequisite the estate does not declare, a required argument the
   provider wants, raw HCL nobody has reviewed are data satz has and the app has no
   other way to learn. It is skipped when the front end already refused, whose message
   the check would only repeat, and when the reload follows a write, whose check has
   just run and whose findings are carried in. The same build runs after every commit
   and on the top bar's Reload; no file watcher runs.

The model is derived, never guessed: a key the schema does not know is `Unknown` and
read-only. `ResourceNode.missing_required` lists the schema's required attributes and
blocks the node lacks, minus what satz derives from the position. An `AttrRow` is
locked when it is `import-id`, computed-only, or inside an `Unknown` block; a value
carrying an interpolation, a reference or an object is edited in `EditMode::Source`. A
`ParamRow` joins its question. `answer_kind` is the shape an interview answer is typed
in, read off the questions report: the `shape` satz publishes per row — what the pack
declares the param with — else the shape of the value the interview offers, which is
the order satz's own `parse_answer` reads an answer in. The app derives no shape of its
own; two derivations of one fact disagree one day, and then the app writes an answer
satz refuses. satz offers no empty value, so a param a pack declares `[]` offers nothing
and is still a list; a param declared as a map has no typed field, since satz answers
one only by an edit to the estate's params. `tests/e2e_answer_shapes.rs` holds every
question of every pack under `vendor/satz/presets` to its declaration, read from the
pack's own text — satz's `shape` and that reading are two readings of one fact, and the
test is that they agree. A param that is a gate in satz's pack report, and every option
of a `oneof`, is no param row: the Packs view switches the one, the Decisions view
answers the other.

The packs are satz's, not the model's. `EstateModel::packs` is the `PacksReport`
`satz_packs` returned — every node of the pack graph satz ships with the presets, the
estate's line for it, whether it deploys, what it needs and what needs it, and the
compile's findings about it — and `EstateModel::phases` is the comment block above each
`use` line, active or commented, by its line number: the one thing the view reads from
the file, to put a pack under the phase its line stands under. The app keeps no pack
table, parses no pack file and derives no dependency; what a pack needs is a
`Requirement` satz states, with whether it is met, and a pack the view hangs below
another is one whose requirement that other pack meets alone ([ADR
0018](adr/0018-the-packs-view-shows-satzs-pack-graph.md)). A `use` of a file the graph
does not know is `unmanaged`, and its gate, where it has one, is a param like any other.
`tests/e2e_pack_edges.rs` holds the model's report to what `satz packs --format json`
writes for the same estate, on the skeleton as written and interviewed with packs on.

A pack the operator writes is judged the same way: by satz. The Packs view runs `satz
review-pack` over a pack file anywhere on disk, through the CLI with the estate's
config, since `satz mcp` is confined to the estate's root ([ADR
0019](adr/0019-the-pack-review-runs-the-cli-and-places-a-private-pack-as-a-local-fork.md)).
The review — the bytes it judged and satz's report — is kept in the estate store, not
the model: a reload leaves it, and the drawer shows its findings beside the estate's
own until the review is closed. Placing the pack privately writes the reviewed bytes
into `presets_dir` as `<stem>.local.satz` under the write lock, checks the estate with
it there and removes it on a refusal; it never writes over a file of that name that
holds other text.

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
   not bind is appended before its `}` as satz's `bind` appends it, and the params block
   is then laid out by `satz_core::fmt::format`, the rest of the file untouched — what
   `bind` does after an append, so both write the same bytes. The proof is in two
   parts: `satz_core::satz::parse` must accept the new text (`EditError::Syntax`), and
   the new tree must equal the old one in a walk of node signatures where each edited
   value stands as a hole and each appended param as one entry; any other difference
   is `EditError::ChangedElsewhere` naming the line. The layout is held to the same
   walk against the text before it. Two edits on one node, one inside another, and two
   appends of one name are refused.
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
`CompileSummary`; `CliChecker` runs `satz --config <dir> transpile <path> --check
--format json`, which prints the same `CompileSummary` on stdout and exits 1 on a
refusal. Both read satz's findings as data rather than the sentences it renders them
to — one diagnostic per finding, at the file and line it names, with its `kind` — and a
finding's relative `file` resolves against the estate's directory, the one `--config`
names. A structured payload of a shape the app does not read is `CheckFailure::Failed`,
never a guess, and so is a zero exit whose stdout is not a summary. A refusal that
never reached the compile carries no findings, and its text — what satz printed as
`error: …` — is read as satz's output. A test holds both to the same verdict, the same
summary and the same `(file, line, kind, message)` set.

What a check that passed reported reaches the drawer too: after a write lands — a
value edit, an answer, a pack switched — `Committed.summary.findings` becomes diagnostics
at their lines, so a warning satz raised is visible beside the change that raised it.

A delegated write is satz's own writer on the real file: an answer or a `oneof` choice
is `satz_interview`, a pack switched on is `satz_add_pack` — its gate bound true, an
option's siblings false, its line uncommented or written where the pack graph places
it, the map's as much as any other — and a pack switched off is `satz_remove_pack`,
which binds the gate false and leaves the line. `Snapshot::take(path)` records the bytes
first; `Snapshot::verify(&dyn Checker)` then checks the real path and is `Committed` on
a pass; a refusal, or a checker that could not run, writes the recorded bytes back. A
tool that refuses — a switch while a pack it needs is off, or while a pack that needs it
is on — wrote nothing; its sentence is a toast and a `DiagSource::Tool` diagnostic in the
drawer, beside what the reload's own check says of the file.
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
  a `ToolDef`, `input_schema` verbatim; a tool whose output schema names properties gets
  `Returns JSON with the keys {…}. A refusal is prose instead, marked as an error.`
  appended to its description, because a satz refusal is a sentence with `isError`
  whatever the schema says. `body()` in `types.rs` owns the four breakpoints (the last tool, tools
  sorted by name; `system[0]`; the last block of the last user message; a marker set
  anywhere else is dropped) and sends adaptive thinking with a summarised display,
  `output_config.effort`, and `fallbacks: "default"` when asked
  ([ADR 0009](adr/0009-refusal-fallbacks-are-on-by-default.md)).
- **Stream.** `ClaudeClient` posts to `{base_url}/v1/messages` with `x-api-key` or a
  bearer token and the betas a request needs in one `anthropic-beta` header.
  `SseDecoder` and `Assembler` fold the events into a `Response`, and every complete
  block arrives as `StreamEvent::BlockStop { index, block }`. A tool's input arrives as
  `input_json_delta` fragments that are concatenated as they come — each forwarded as
  `StreamEvent::ToolInputDelta` for display, none parsed — and parsed once, at
  `content_block_stop` (`sse::tool_input`). A buffer with nothing in it, which is what a
  tool called without arguments leaves (no fragment, or one empty `partial_json`), is the
  input the block opened with, `{}`. A buffer with something in it that is not JSON is
  `ClaudeError::Stream` naming the tool and showing the buffer, quoted, whole up to 240
  characters and otherwise its first 160 and last 80, with its length in bytes.
  `OpenAiCompat` and `Ollama` close their tool calls by the same function. A request is retried at most twice, on a
  rate limit, an overload, a server error or a connection failure, and only before the
  first byte of its stream. `ContentBlock` types the five block kinds the code reads;
  every other block is `ContentBlock::Other`, carried and replayed verbatim.
- **Tool calls.** On `StopReason::ToolUse` every `ToolUse` block runs in order. The
  gate is `runs_without_asking`
  ([ADR 0005](adr/0005-tool-approval-by-mcp-annotation-and-the-capability-ceiling.md)):
  a read-only tool runs; a non-destructive tool runs when `auto_approve_writes` is set
  or the operator allowed it for the session; a destructive tool asks every time.
  Anything else raises `AgentEvent::ToolCallPending` and waits for `Approval::Once`,
  `ForSession` or `Deny`. `Deny`, a tool the host does not list, an input that is not a
  JSON object, and a call whose parameters satz refuses as a JSON-RPC `invalid_params`
  error (`SatzError::InvalidParams`, satz's message as the text) are each a
  `tool_result` with `is_error`, and the turn goes on; any other failure below the tool
  is `ClaudeError::Tool` and fails the turn. `result_text` (`bridge.rs`) makes the
  block's content: a result's structured payload pretty-printed, else its text; a
  refusal leads with satz's sentence, then the structured part it carries, if any — a
  refused `satz_transpile_check` hands over its `CompileSummary` — after a blank line.
  All results of one assistant message go back in one user message.
- **Events of a request.** Every request opens with `AgentEvent::Started { model }`,
  the model the server names in `message_start` — another than the one asked for when
  the server fell back — and ends with `AgentEvent::RequestDone { usage }`, that
  request's usage alone.
- **Ends.** `EndTurn`, `MaxTokens` and `StopSequence` are `AgentEvent::TurnDone`, whose
  usage is every request of the turn summed (`Usage::plus`);
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
- **The chat's debug log.** `ChatStore.debug` holds one `DebugEvent` per tool call of
  the conversation, under its call id, whatever the panel shows: opened by
  `ToolUseStarted`, given the input by `ToolCallPending` or, for a call that asked
  nobody, by the input the stream carried, and closed by `ToolResult` with the text
  `result_text` made, `is_error` and the milliseconds. The chat coroutine follows the
  estate session's `mcp_stderr()` and gives each line to the running turn's first call
  still without its result; the API engine runs a response's calls in order, so that
  is the call running. A replayed transcript fills the log from its `tool_use` and
  `tool_result` blocks. On the Claude Code engine the calls run on Claude Code's own
  `satz mcp`, whose stderr does not reach the app, and the entry says so.
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
- **The environment** sets `MAX_MCP_OUTPUT_TOKENS` to `session::MAX_MCP_OUTPUT_TOKENS`
  (100,000). Above its limit Claude Code cuts a tool result into text that is no longer JSON
  or saves it to a file, which a session without built-in tools cannot read; satz's largest
  result, `satz_report_compliance` for every framework the estate is held to, is about
  19,000 tokens per framework.
- **The protocol.** The client writes one JSON object per line on stdin: an initialize
  control request at spawn, then `{"type":"user",…}` per turn, a `control_response` per
  approval, and an `interrupt` control request to cancel. The CLI writes one per line
  on stdout: `system/init` (the session id, the model, the MCP servers' status — read
  at the first turn, not at spawn), `stream_event` carrying the Messages API's own
  events, `user` messages holding the results of the tools it ran, `rate_limit_event`,
  `can_use_tool` control requests, and a `result` line ending the turn.
- **The translation.** `stream_event` payloads go through the same `Assembler` the API
  engine uses — a new one per `message_start`, since one turn is many assistant
  messages, and the tool input rule of section 4c with it — and its `StreamEvent`s become `Started { model }`,
  `TextDelta`, `ThinkingDelta`, `ToolUseStarted` and `ToolInputDelta` under the name satz
  gives the tool, with the `mcp__satz__` prefix stripped, and `RequestDone { usage }` at
  each message's end. A `tool_result` block becomes `ToolResult`, its `millis` measured
  from the call's input being complete, or from the operator's answer when the call
  raised a card, to the result; a `can_use_tool` request becomes `ToolCallPending` whose
  answer is the control response (`ForSession` remembers the tool, so the next call needs
  no card), `rate_limit_event` becomes `AgentEvent::Notice` for the footer, and `result`
  becomes `TurnDone` — `EndTurn`, or `MaxTokens` for `error_max_turns` — with the
  `result` line's usage, Claude Code's total over the turn.
- **The estate's write lock is held for the whole turn**, not per call: Claude Code's
  satz server writes the estate, and the app cannot see the calls it pre-approved. The
  app keeps no transcript for this engine — Claude Code holds the conversation, "New"
  starts a fresh process, and the model comes from Settings because it is an argument of
  that process.
- **A failure** is the session's `ClaudeCodeError`, returned from `run_turn` and sent as
  `AgentEvent::Failed` through the one conversion into `ClaudeError::ClaudeCode`, which
  carries the error's own message: a stream the assembler refused reads `claude code:
  stream: …`, with the prefix once.
- **The stream log** ([ADR 0013](adr/0013-the-claude-code-stream-log-is-verbatim-off-by-default-and-bounded.md)).
  With `Settings.claude_code_log` on, `SessionOptions.log` names
  `<data dir>/satz-studio/logs/claude-code/` and the session writes one file per process,
  `<created>.log`; with it off, which is the default, nothing is written. A record is one
  line, `<RFC 3339 instant>\t<channel>\t<line>`: `stdout` every line the CLI writes,
  recorded before it is parsed; `stdin` every line the app writes; `stderr` every line
  of the CLI's standard error, read on its own task; `studio` the app's own — the header
  (the app and CLI versions, the binary, the estate, the command line) and, when a spawn
  or a turn ends in an error, that error. Nothing is redacted. A file stops recording at
  16 MiB with a last line saying so, and opening a log deletes the oldest so that ten
  remain. A write that fails fails the turn, naming the file. `awk -F'\t' '$2 ==
  "stdout"' <file> | cut -f3-` gives the stream back as the CLI wrote it.

## 5. Deployment and CI

`Dioxus.toml` names the bundle identity and the two files every bundle carries,
`LICENSE` and `NOTICE` (`bundle.resources`, which dx resolves against the directory it
runs in — the repository root); `dx bundle --release --platform desktop` produces the
bundle per OS, and U10 builds the release workflow that runs it. The
webview is a runtime dependency: WebView2 on Windows, `webkit2gtk-4.1` on Linux.

`.github/workflows/ci.yml` runs `core` on `ubuntu-24.04` once per commit — on a pull
request for a branch, on the push for `main`, since a branch's pull request already runs
the jobs on the branch merged into `main`: the desktop crate's system libraries, then `scripts/install-satz.sh`, then
`cargo fmt -p satz-studio-core -p satz-studio -- --check` (the two packages;
`vendor/satz` is satz's own), `cargo clippy --workspace --all-targets --locked -- -D
warnings`, `cargo test --workspace --locked` and `cargo build -p satz-studio --locked`.
`install-satz.sh` downloads the cargo-dist installer of the newest satz release with
its SHA-256 sidecar, refuses an installer without one, and refuses a release below
`MIN_SATZ`; a tag given as its one argument installs that release instead. It follows
the newest because satz keeps only its five newest releases, so an installer asset
pinned by tag is gone within days, and a red run the day satz releases is the alarm that
the app has fallen behind. That is why nothing refuses a newer satz and no test that
locates the real binary asserts it is at the build's satz exactly. It
also writes the runner's satz config (`self_update_frequency = "never"`) when none
exists, so no update check reaches GitHub while the tests drive satz. `platforms` (`macos-15`,
`windows-2022`) runs the same formatting, clippy, test and build steps on the same
events; on Windows a PowerShell step does what `scripts/install-satz.sh` does, through
satz's `satz-installer.ps1`: the sidecar check, the run the app makes (`powershell -File`,
`SATZ_INSTALL_DIR`, `SATZ_NO_MODIFY_PATH=1`), the `MIN_SATZ` floor and the satz config. macOS is Apple silicon alone: `macos-15-intel` is the most expensive
runner in the catalogue and ran the same code on the same OS beside `macos-15` — the
difference is the architecture, and nothing here is architecture-dependent, the webview,
the keyring and the satz binary being the platform's rather than the chip's.
`release.yml` does not build it either: macOS is Apple silicon in both, and an Intel Mac
gets no bundle. Linux and Windows are unchanged and x86_64 — this is about the Mac. `.github/workflows/names-gate.yml` runs `scripts/check-names.sh` over
the tree and over the commits a pull request adds, or the push to `main` that merges it.

## 6. Decisions

| record | decision |
|---|---|
| [0001](adr/0001-dioxus-desktop-on-the-webview-renderer.md) | Dioxus 0.7 desktop on the webview renderer |
| [0002](adr/0002-a-separate-repository-with-satz-pinned-once.md) | a separate repository; satz pinned once, as the submodule; the binary required at `MIN_SATZ` |
| [0003](adr/0003-the-document-layer-is-the-tree-sitter-grammar.md) | the document layer is the tree-sitter grammar, vendored and compiled in; satz-core stays the authority on meaning |
| [0004](adr/0004-claude-natively-other-providers-adapt-into-its-message-model.md) | Claude natively: the Messages API wire types are the app's message model; other providers adapt into it |
| [0005](adr/0005-tool-approval-by-mcp-annotation-and-the-capability-ceiling.md) | tool approval by the MCP annotations satz declares; the capability ceiling stays satz's |
| [0006](adr/0006-apply-and-bootstrap-run-in-the-users-terminal.md) | `apply` and `bootstrap` run in the user's terminal, never with `-auto-approve` |
| [0007](adr/0007-pack-rows-are-derived-from-the-estate-file.md) | superseded by 0018 — pack rows derived from the estate file, the questions report and the resolved params |
| [0008](adr/0008-transcripts-live-outside-the-estate.md) | transcripts live under the app's data directory, never inside an estate |
| [0009](adr/0009-refusal-fallbacks-are-on-by-default.md) | refusal fallbacks are on by default, off by a Settings switch |
| [0010](adr/0010-claude-code-as-the-subscription-backend.md) | Claude Code as the subscription backend: the installed CLI driven over stdio, the estate's satz MCP server, the app's own approval card |
| [0011](adr/0011-the-licence-is-apache-2-0.md) | the licence is Apache 2.0, with `NOTICE` for the material bundled under other terms |
| [0012](adr/0012-migrate-hands-off-to-the-terminal.md) | `migrate` hands off to the terminal with `apply` and `bootstrap`; `bootstrap --dry-run` is a check that runs in the app |
| [0013](adr/0013-the-claude-code-stream-log-is-verbatim-off-by-default-and-bounded.md) | the Claude Code stream log is verbatim, off by default, one file per conversation, and bounded to ten files of 16 MiB |
| [0014](adr/0014-a-newer-satz-is-a-notice-and-the-app-looks-for-releases.md) | a satz newer than the build runs and is a notice, not a gate; the app looks for releases of itself and of satz once per launch and says so in the title and the top bar |
| [0015](adr/0015-the-packs-view-draws-the-dependency-tree-the-packs-declare.md) | superseded by 0018 — the dependency tree the packs declare with `ask_when`, drawn as tree blocks in the grid with connectors in CSS |
| [0016](adr/0016-macos-is-apple-silicon-alone.md) | macOS is Apple silicon alone, in CI and in the release; Linux and Windows stay x86_64 |
| [0017](adr/0017-a-release-is-cargo-release-on-main.md) | a release is `cargo release` on `main`: the version bump is the one commit that lands without a pull request |
| [0018](adr/0018-the-packs-view-shows-satzs-pack-graph.md) | the Packs view shows satz's pack graph (`satz_packs`) and switches a pack with `satz_add_pack` and `satz_remove_pack`; the app derives no pack row and no dependency |
| [0019](adr/0019-the-pack-review-runs-the-cli-and-places-a-private-pack-as-a-local-fork.md) | the pack review runs `satz review-pack` through the CLI with the estate's config; a private pack is placed as `<stem>.local.satz`, never over other text; upstream is a pull request by hand |

## 7. Not built, and why

- **No embedded terminal.** `apply` and `bootstrap` hand stdio to tofu and to the
  human; tofu's approval prompt is the safety step. `EstateSession::external_command`
  writes a one-shot script under `<data dir>/satz-studio/run/` holding `cd "<estate>"
  && "<satz>" --config . <args…>`, and `open_in_terminal` opens it with the OS
  terminal. The app learns nothing from the terminal; `-auto-approve` is never passed.
- **No pack logic of the app's own.** Which packs an estate uses, what each needs and
  what a switch writes are satz's pack graph, read through `satz_packs` and changed
  through `satz_add_pack` and `satz_remove_pack`. The app keeps no table of packs, reads
  no pack file for its dependencies, and writes no pack line itself.
- **No second parser.** The document layer is the tree-sitter grammar and meaning is
  `satz_core::satz::parse`. A grammar gap is fixed in the grammar repository.
- **No markdown renderer in the chat.** The model's text is shown as text; the tool
  calls, their results and the approval cards are the structure.
- **No merge.** A file that changed on disk under an edit is refused and reloaded.
