# satz-studio architecture

What satz-studio is built of and how its parts run, for people changing it. The
decisions behind the shape are in [`adr/`](adr/README.md). Where a part is not built,
the section names the unit that builds it.

## 1. Goal

satz-studio is a desktop app over [satz](https://github.com/tjirsch/satz). It runs the
interview satz's packs declare, edits the pack choices (the estate map) and the values
in the estate files through typed fields, runs satz commands, and sets an external agent
up on the estate and starts it. It runs no model itself
([ADR 0020](adr/0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md)).
Version one edits what exists: an answer, a pack choice, an attribute value. Adding and
removing resources and blocks is not in it.

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
  newer satz, because CI installs the NEWEST satz release: on the day after a
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
  gate on every commit. The app holds no credential and keeps no conversation: what it
  writes outside an estate is the settings file and the one-shot scripts it opens in the
  terminal.
- **Offline tests.** Every unit tests against `tests/fixtures` and the pinned
  `vendor/satz`; the tests that drive the `satz` binary need it installed. Nothing needs
  a network or a credential.

## 3. Building blocks

Two crates in one workspace, satz pinned once as the submodule `vendor/satz`
([ADR 0002](adr/0002-a-separate-repository-with-satz-pinned-once.md)).

### `crates/satz-studio-core`: the headless half

| module | responsibility | main public types |
|---|---|---|
| `src/estate.rs` | an estate directory as satz sees it: `config.toml` read into `ToolConfig` with satz's defaults — an omitted `yaml_dir` is `satz`, `include_dirs` `[".", "satz"]`, each held to satz's `src/settings.rs` at the pinned submodule by a test — and resolved against its own directory; `discover` walks a folder for every `config.toml` (depth 6, at most 200, skipping `hcl/`, `target/`, `evidence/`, `node_modules/` and dot-directories); `estates` lists the `.satz` files in `yaml_dir` that declare an `estate`, skipping a checked temp file (`is_checked_temp`); `loader` resolves `use "…"` as satz does (the file's directory, then `include_dirs`); `params` and `deployment_mode` read the resolved params without a schema, and `acknowledged` asks satz's own rule whether those params acknowledge a pack's notice; `HclState::read` answers two facts about `hcl_dir` and no more — `main.tf` is there, so the estate has been transpiled here, and `.terraform` is there, so the tool's init has run | `EstateDir`, `ToolConfig`, `EstateError`, `HclState`, `declares_an_estate` |
| `src/cst/` | the lossless document layer over the vendored tree-sitter grammar ([ADR 0003](adr/0003-the-document-layer-is-the-tree-sitter-grammar.md)); `grammar.rs` exposes the compiled parser, `build.rs` walks the tree into nodes with byte spans, `uses.rs` and `render.rs` read the pack lines and write values | `Cst`, `Node`, `NodeKind`, `Span`, `UseLine`, `UseState`, `TypedValue`, `StyleCtx`, `scan_uses`, `render_value`, `style_of`, `grammar::language` |
| `src/edit/` | the edit primitives and the write discipline (section 4b): `apply.rs` the splice and its proof, `commit.rs` the temp file and the rename, `check.rs` the two checkers, `snapshot.rs` the delegated write | `Edit`, `EditSession`, `Proposed`, `Committed`, `Rollback`, `Checker`, `CheckFailure`, `McpChecker`, `CliChecker`, `Snapshot`, `Delegated`, `NotLanded`, `Cause`, `Restore`, `sha256_hex` |
| `src/schema.rs` | the provider schema as `satz update-schema` writes it, the types lifted from satz's `src/schema.rs`; `load_all` reads every `*.json` in `schema_dir` and is `SchemaError::Missing` for a directory that is absent or holds no resource type; `AttrType` decodes Terraform's type expression and prints it in Terraform's spelling | `ResourceRegistry`, `AttrType`, `BlockSchema`, `AttributeSchema`, `SchemaError` |
| `src/model/` | the view model, built pure and rebuilt after every commit and reload: `outline.rs` classifies the blocks as satz's `EstateResolver` and `is_child` do, `params.rs` joins the `params { }` block with the questions and holds `answer_kind`, the shape an interview answer is typed in — satz's own, read off the report, `value.rs` decodes a string as satz's lexer reads it, and `hcl_blocks` reads the file's `hcl` statements with whether each carries a `trust` reason. The packs are satz's `PacksReport`, carried as `satz_packs` returned it, with the phase comment above each `use` line by its number ([ADR 0018](adr/0018-the-packs-view-shows-satzs-pack-graph.md)) | `EstateModel`, `ResourceNode`, `ResourceKind`, `AttrRow`, `ParamRow`, `ParamKind`, `SourceValue`, `StrPart`, `EditMode`, `HclBlock`, `SchemaStatus`, `answer_kind` |
| `src/git.rs` | what `satz merge-presets` needs from git: it edits the estate file in place and asks `git status` in the estate file's directory for the undo, refusing outside a work tree or without git. `WorkTree::read` asks `git rev-parse --is-inside-work-tree` in that directory — a repository above it counts — and answers `Inside`, `Outside` with git's own words, or `NoGit`; `init_steps` are `git init -b main`, `git add -A` and one commit naming the estate; `run` streams one git command's lines and is cancellable | `WorkTree`, `GitError`, `init_steps`, `run` |
| `src/satz/binary.rs` | where satz is and which version: the Settings override, `PATH`, `~/.local/bin/satz` (`satz.exe` on Windows — `home_bin_dir` and `in_dir`, the folder the installer writes too); the gate that refuses a satz older than `MIN_SATZ`, the oldest satz this build works with; `built_against` is the submodule's satz version, and `ahead_of_build` reads a satz past it as a patch or a minor ahead and refuses nothing | `SatzBinary`, `MIN_SATZ`, `Ahead` |
| `src/satz/self_update.rs` | what `satz self-update --check-only` printed, read narrowly: the `Latest version:` line against the version of the satz asked, and the `Release:` line when there is one — an output without the line is an error quoting it; `unprompted_checks_allowed` reads `self_update_frequency` from the operator's `~/.config/satz/satz.toml` as satz reads it, a missing file or key being `always` and a file that does not parse an error | `SatzRelease`, `read_check`, `unprompted_checks_allowed` |
| `src/satz/install.rs` | satz's own cargo-dist installer, for an operator with no satz and, on Windows, for an update: `Installer` names the two — `satz-installer.sh` under `sh`, `satz-installer.ps1` under `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File` — with their `.sha256` sidecars, and `for_this_system` picks one. The installer and its sidecar are the assets of ONE `releases/latest` object, so a release published between two downloads cannot pair them; `VerifiedInstaller::verify` is the only way to hold the script, and only on a matching SHA-256; `run` writes it under its asset name into a private temporary directory and runs it with `SATZ_INSTALL_DIR` naming the folder the caller gives, `SATZ_NO_MODIFY_PATH=1` (the installer otherwise adds its folder to `PATH` in the shell profiles or the Windows user `PATH`, and `locate` searches `~/.local/bin` without it) and stdin closed, streamed and cancellable | `Installer`, `VerifiedInstaller`, `InstallError`, `fetch_verified` |
| `src/satz/export.rs` | the decisions sheet and the workbook: `formats` reads the values of `--format` from the installed satz's `satz questions --help` — the `Possible values:` block, each with satz's own line — and a help without one is an error, never an empty list; `args` is `questions <estate> --format <format> --out <file>` and nothing else; `destination` gives a chosen path the format's extension when it has none, so the file the app opens is the file satz wrote; `written` refuses a file that is missing or empty after a clean exit | `QuestionsFormat`, `formats`, `parse_formats`, `args`, `destination`, `written` |
| `src/satz/cli.rs` | `satz --config <dir> <args…>` in the estate's directory, stdout and stderr streamed line by line and cancellable, by the one streaming helper the installer's run shares; `help` reads a command's long help; `json_report` runs a reporting command with `--format json` and an `--out` of its own and types the file it wrote; `json_verdict` does the same for a command whose exit status is a verdict on what it judged (`review-pack`), returning the report with the status, and a non-zero exit that wrote no file is an error; `run_in` is the same streaming without a `--config`, in a working directory of its own, for the one command that runs before a `config.toml` exists; `output` runs a command to its end with both pipes held whole, for one whose whole output is the answer, and `help` is that call with `--help` | `SatzCli`, `CliLine`, `Output` |
| `src/satz/import.rs` | `satz import` as a typed thing: `plan` decides from the directory alone whether `satz init` runs first; `ImportShape` picks the flags and `ImportOptions::argv` renders only the chosen shape's; `satz_files` / `written_since` read back what the run wrote; `ImportReport` splits satz's report into sections | `ImportShape`, `OnCollision`, `ImportOptions`, `ImportPlan`, `Written`, `ImportReport` |
| `src/satz/init.rs` | `satz init` as a typed thing: `InitOptions` renders the flags it was given to argv and passes nothing for a field left blank, so a blank field is the instruction to derive — or, for `--workload-folder-name`, satz's own `""`, the organisation; `check_target` refuses a directory that is not there or already holds a `config.toml`; `created` reads what a finished run left, because `init` names the estate file after a customer id it may have derived and the name is not knowable in advance | `InitOptions`, `check_target`, `created` |
| `src/satz/mcp.rs` | one `satz mcp` child per estate, spoken to with rmcp over stdio; every rmcp type stays inside this file. A tool's refusal is a `ToolOutcome` with `is_error`; a JSON-RPC `invalid_params` error in place of a result — a tool name satz does not serve — is `SatzError::InvalidParams` naming the tool | `McpSession`, `ToolOutcome` |
| `src/satz/session.rs` | one session per open estate: the CLI runner, the MCP child rooted where `satz mcp-config` roots a client's server and started at `Allow::STUDIO`, the write lock every writer takes, the identity from `satz_open`; `apply` and `bootstrap` as a one-shot script in the OS terminal, its paths single-quoted for `sh` (`sh_path`) and double-quoted with `%` doubled for `cmd.exe` (`cmd_path`) | `EstateSession` |
| `src/satz/reports.rs` | serde mirrors of what a reporting command writes with `--format json` and satz returns as `structuredContent`: unknown fields ignored, missing required fields fail; the questions report round-trips a recorded output of the pinned satz. `Finding` is satz's own list of what the compile found after the front end — a `CompileSummary` carries the warnings and infos it did not refuse on, a `Refusal` the ones it did; `kind` is the kebab-case word satz writes, kept as a `String` so a kind satz adds is carried instead of failing the result. `NoticeRow` is what a pack asks to be run once it is on, with the param that acknowledges it; `severity` is required and typed — `error` is the one that holds up every command writing to the organisation — so a notice without one, or with a word satz adds, fails the report rather than reading as one that holds nothing up, and an interview report without `notices` fails too. A `QuestionRow` carries `required` for a choice and `empty` for a question that says what `""` means; a choice that is not required is answered `NO_BRANCH` (`none`, every option `false`) as well as by an option, `bound_option` and `bound_label` read which one an answered choice carries, and `shown` writes an empty answer with its meaning beside it, as satz does. `PacksReport` is `satz_packs`: one `PackRow` per node of satz's pack graph — its role, gate, answer, default and value, where its `use` line stands (`PackLine`, at its line number), whether it deploys, what it `requires` (each a `Requirement` with `met`), what it is `required_by` and `excludes`, its notices, what it `contributes` to another pack's list params and the compile's findings about it — the `use` lines the graph does not know, and the findings with the pack as `subject` and the command that answers each as `fix`. Every field satz always sends is required, and a line state satz adds fails the report; it round-trips a recorded `satz packs --format json` of the smoke estate. `AddPackArgs`, `RemovePackArgs` and `PackChange` are the arguments and the result of `satz_add_pack` and `satz_remove_pack`. `MergeReport` is `satz_merge_presets`: its events (`MergeEvent`, tagged by `kind`; an event kind satz adds fails the report, an outcome word satz adds is carried), its `MergeCounts`, `attention` and the notices it opened; `lines()` is the report as the command log shows it, in sentences; it round-trips a recorded merge of the smoke estate. `PackReview` is `satz review-pack --format json`: the pack, the estate it was folded into, what it emits and its findings, every field required; `passed()` is satz's verdict — no finding is an error; it round-trips two recorded reviews of the pinned satz, one clean and one broken | `QuestionsReport`, `QuestionRow`, `InterviewArgs`, `InterviewReport`, `NoticeRow`, `PrerequisitesResult`, `OpenReport`, `CompileSummary`, `Finding`, `FindingSeverity`, `Refusal`, `PacksReport`, `PackRow`, `PackRole`, `PackLine`, `Requirement`, `RequirementKind`, `Unmanaged`, `AddPackArgs`, `RemovePackArgs`, `PackChange`, `MergeReport`, `MergeEvent`, `MergeCounts`, `PackReview` |
| `src/satz/review.rs` | `satz review-pack` and the two places a reviewed pack goes ([ADR 0019](adr/0019-the-pack-review-runs-the-cli-and-places-a-private-pack-as-a-local-fork.md)): `review` runs `satz --config <estate dir> review-pack <pack> [--against <estate>] --format json` through `json_verdict`, holds the exit status to the report's own verdict (`SatzError::Verdict` when they disagree) and keeps the bytes it judged, refusing a pack that changed while satz read it; `diagnostics` is each finding at its `file:line` from `satz review-pack`. `local_name` is `<stem>.local.satz` — a `.local.satz` keeps its name, a `.diff.satz` is refused — and `upstream_name` is `presets/<stem>.satz`. `place_private` writes the reviewed bytes into `presets_dir` under that name: refused when the pack changed since its review or the library is missing, nothing written when the file holds these bytes already, refused when it holds anything else; the file is created with `create_new`, the estate is checked with it in the library, and a refusal or a checker that could not run removes it again | `ReviewedPack`, `review`, `review_args`, `diagnostics`, `local_name`, `local_target`, `upstream_name`, `place_private`, `Placed`, `PlaceError` |
| `src/satz/mcp_config.rs` | `satz mcp-config <estate> --client <client> --allow <ceiling>`, which is where the configuration an agentic client reads comes from: `args` is that argument vector, with `--write` for a write and `--write --force` for the one refusal that asks for it; `run` hands back what satz printed, the block on stdout and its notes on stderr; `outcome` is the line a write ended on, for the toast; `refusal` is satz's own stderr where satz refused, unreworded; `force_would_answer` is whether satz's refusal is the one `--force` answers, by the sentence satz prints; `server` reads the one server of the printed block and `root` its `--root`, the root the app's own session takes ([ADR 0022](adr/0022-the-mcp-root-is-the-one-satz-mcp-config-renders.md)); `target_file` is the file satz's notes say a write goes to, and `written` reads satz's key there against the printed entry — absent, the same, another entry, or unreadable ([ADR 0021](adr/0021-the-settings-ceiling-is-the-agents-and-studio-writes-at-its-own.md)) | `Client`, `Run`, `Printed`, `Server`, `OnDisk`, `Written`, `args`, `run`, `outcome`, `refusal`, `force_would_answer`, `server`, `root`, `target_file`, `written` |
| `src/satz/mod.rs` | the capability ceiling — `Allow::STUDIO`, `read,write`, is the app's own session's — and the one error type of the driver; `SatzError::Printed` is satz output the app does not read, quoted | `Allow`, `SatzError` |
| `src/agent.rs` | starting the agentic client on the open estate ([ADR 0020](adr/0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md)): `locate` is the configured client's first word on `PATH`, told apart from a line naming none, and `start` runs it in the estate's directory through the one-shot script the terminal hand-off uses, the directory quoted as that script quotes it. The configuration that points it at the estate is satz's, `src/satz/mcp_config.rs` | `AgentError`, `DEFAULT_AGENT_COMMAND`, `locate`, `program`, `script_text`, `start` |
| `src/github.rs` | the latest release of a repository through GitHub's unauthenticated REST API, always `releases/latest` and never a tag; `look_for_studio_update` compares satz-studio's with the running version and downloads nothing; a 403 or 429 from the API is `RateLimited` with the reset, a connection that fails is `Unreachable`, a 404 is `NoRelease` | `Release`, `Asset`, `GithubError`, `StudioUpdate`, `latest_release`, `download`, `look_for_studio_update` |
| `src/settings.rs` | `<config dir>/satz-studio/settings.toml`: a missing file is the first run, a broken one is an error; six fields and no credential among them — the satz path, `dismissed_satz` (the satz release newer than the build whose notice the operator dismissed, which permits and refuses nothing), the MCP ceiling an agent's configuration is written with, `agent_command`, the theme and the folder the Start screen opened last; `data_dir` is where the one-shot scripts go | `Settings`, `Theme`, `settings_path`, `data_dir` |
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
  `LookForStudioUpdate`, `InstallSatz`, `CancelInstall`, `SaveSettings`; at startup it
  locates satz, makes the two release looks and walks `last_root`;
- **one coroutine per open estate** (`src/state/estate_actions.rs`, `EstateAction`),
  started by `EstateHost` in `src/shell/mod.rs` with the `Arc<EstateSession>` and
  living as long as the estate is open: `Reload`, `RunCommand`, `RunNoticeCommand`,
  `CancelCommand`, `OpenInTerminal`, `Answer`, `AcceptDefaults`,
  `WritePrerequisites`, `CommitEdit`, `AddPack`, `RemovePack`, `MergePresets`,
  `ReviewPack`, `PlacePrivate`, `CloseReview`, `InitRepository`, `Export`, `Close`,
  `Switch`.

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
**Decisions**, **Estate**, **Checks**, **Deploy**, then Agent and Settings at the foot of
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
installed satz's own set; a help the app cannot read is a toast and the card's text. Commands is not a
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
file of the app's own.

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
`--output` and `--verbose` for a state document or a live scope, `--organization` for a
state document and Terraform HCL — satz writes no estate from a state or a configuration
that names no organisation, nor from any `--wrap-all` import, without it, and refuses it
on a live sweep, which reads the organisation from its root — and `--wrap-all` for
Terraform HCL — never a flag of another shape. `--into` is not among them: importing only what an open estate
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
reason for each, its warnings, and the rest, in order. The warnings are satz's
`warning:` and `error:` lines and the two losses it reports as `import:` blocks, each
with the indented lines under it: the attributes the provider schema names that the
estate does not carry, which an apply would reset, and the asset types Cloud Asset
Inventory does not serve, of which nothing is in the estate. It rewrites no line and drops
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
3. `EstateSession::open(bin, dir, main)` resolves `main` as satz resolves a name on the
   command line, makes it absolute, and asks satz for the root: `satz --config <dir>
   mcp-config <main> --client claude-code --allow read,write` prints the server a client
   starts, and its `--root` — the directory holding `config.toml`, canonicalised — is the
   root (`mcp_config::root`,
   [ADR 0022](adr/0022-the-mcp-root-is-the-one-satz-mcp-config-renders.md)). The window
   and every client it configures work under the same boundary. The session then spawns
   `satz mcp --root <root> --allow read,write` through `McpSession::open`: `read,write` is
   `Allow::STUDIO`, what the window's own writes need, whatever ceiling Settings holds for
   the agent ([ADR
   0021](adr/0021-the-settings-ceiling-is-the-agents-and-studio-writes-at-its-own.md)). An
   estate file outside its config directory is refused by satz at `satz_open`, naming
   the root.
4. `McpSession::open` initializes and calls `satz_open {config, estate}` for the
   `OpenReport` with `runs_as` and `deployment_mode`. The child's stderr is read to its
   end into a backlog of the last 256 lines. A child that has exited is
   `SatzError::Closed` on the next call; an initialize that fails is `SatzError::Mcp`
   carrying what satz said before it died.
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
satz refuses. satz offers no empty value — except to a question whose `empty` says what
`""` means, where `""` is offered, accepted and counted as the answer — so a param a pack
declares `[]` offers nothing and is still a list; a param declared as a map has no typed field, since satz answers
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
the rename: a view edit, an interview answer, a pack switched. An agent that writes the
estate does it through its own `satz mcp`, outside this window and outside this lock, and
the estate is re-read when the window comes back to the front.

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
which binds the gate false and leaves the line; `satz_update_prerequisites
{report_only: false}` writes the roles and APIs the estate lacks. `Snapshot::take(path)`
records the bytes before the call, and `Snapshot::delegate(call, &dyn Checker)` runs the
call and decides what stands, whatever the call comes to (`Delegated`):

- the call landed: `Snapshot::verify` checks the real path and is `Landed` with the
  `Committed` on a pass; a refusal, or a checker that could not run, writes the recorded
  bytes back and is `RolledBack`;
- satz refused the call (`is_error`), or the call returned no result — the session
  died, the server answered with an error in place of one: `NotLanded` with its `Cause`,
  and `Snapshot::restore_if_changed` compares the file with the record byte for byte. A
  file that differs, or is gone, gets the recorded bytes back (`Restore::Restored`); one
  that is the same is `Restore::Untouched`; a write-back that fails is `Restore::Failed`
  with its path. A tool that refuses has not necessarily written nothing, so the
  comparison runs on every refusal.

`NotLanded::message` is what the operator reads: satz's sentence (or the tool's name
and the error), followed, when the bytes were put back, by "satz refused and had
changed `<file>`; the file is back as it was". It is a toast and a `DiagSource::Tool`
diagnostic in the drawer, beside what the reload's own check says of the file.
`satz_merge_presets` is outside this discipline: it writes the library as well as the
estate file, so a record of the estate file alone cannot put the estate back — restoring
it over a repointed fork would point the estate at the changed upstream pack — and
satz runs it only inside a git work tree, whose history is its undo.
`EstateDir::estates` never lists a checked temp file; `.gitignore` carries the suffix.

### 4c. The agent handoff

satz-studio runs no model
([ADR 0020](adr/0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md)).
The Agent destination (`src/views/agent.rs`) configures an external client on the open
estate and starts it; nothing is remembered between sessions.

- **The configuration is satz's.** `satz mcp-config <estate> --client <client> --allow
  <ceiling>` prints the block that client reads on stdout and its notes on stderr, and
  the app runs it through the CLI runner like any other command
  (`satz::mcp_config::args`, `SatzCli::output`). The binary is the satz that printed it,
  by absolute path, and the root is the estate's own directory: both are satz's to
  resolve, and the app passes neither. The ceiling is `Settings.mcp_allow`, written out
  on every run; it is the agent's alone, and it bounds the satz server, not an agent
  that has a shell of its own
  ([ADR 0021](adr/0021-the-settings-ceiling-is-the-agents-and-studio-writes-at-its-own.md)).
- **What the client runs at is read from its file.** After every run the card reads the
  file satz's notes name (`mcp_config::written`) for satz's key and compares that entry
  with the block satz printed for the setting: not configured, configured at the ceiling
  shown, or another entry with "Replace it" — the `--write --force` run. Saving Settings
  writes no client file.
- **Writing it** is the same command with `--write`: satz merges its own key into the
  file that client reads — `.mcp.json` beside the estate for Claude Code, Claude
  Desktop's own configuration file for Claude Desktop — and leaves every other server in
  it as it is. satz refuses its own key already there with other arguments, and the card
  shows that refusal and offers the `--write --force` run; the app never passes `--force`
  on its own. A file satz cannot parse is refused with no run offered, because `--force`
  does not answer it. Neither file is an estate file, so the write discipline of section
  4b does not reach them — nothing under `yaml_dir` is touched.
- **What satz says is what the operator reads.** `mcp_config::refusal` is satz's stderr
  where satz refused, word for word; `mcp_config::outcome` is the line the run ended on,
  which the toast carries while the card carries the whole of it.
- **The view makes these runs itself**, not through the estate coroutine: they write no
  estate file and take no write lock, so there is nothing for the coroutine's discipline
  to hold. Each card runs the command when it opens and again when the estate or the
  ceiling changes.
- **Starting the client** is `agent::start`: the command line's first word is looked
  up on `PATH` (`agent::locate`), so a client that is not installed is named rather
  than failing out of sight, and the line is then run from a one-shot script in the OS
  terminal, the way `apply` and `bootstrap` are
  ([ADR 0006](adr/0006-apply-and-bootstrap-run-in-the-users-terminal.md)) — the default
  client is a terminal program. Nothing is supervised and nothing is read back.
- **What the agent writes** reaches the window the way an `apply` does: `EstateHost`
  reloads the estate when the window regains focus, and the top bar's reload is there
  for the rest.

## 5. Deployment and CI

`Dioxus.toml` names the bundle identity and the five files every bundle carries,
`LICENSE`, `NOTICE`, `THIRD-PARTY-LICENSES.md` (the licence text of every crate the app is compiled from), `LICENSE-MaterialSymbols` (the icon font's) and `LICENSE-satz-tree-sitter` (the vendored grammar's) (`bundle.resources`, which dx resolves against the directory it runs in — the
repository root — and copies under the file name alone, so no two may share one); `dx bundle --release --platform desktop` produces the
bundle per OS, and U10 builds the release workflow that runs it. The
webview is a runtime dependency: WebView2 on Windows, `webkit2gtk-4.1` on Linux.

`.github/workflows/ci.yml` runs `core` on `ubuntu-24.04` once per commit — on a pull
request for a branch, on the push for `main`, since a branch's pull request already runs
the jobs on the branch merged into `main`: the desktop crate's system libraries, then `scripts/install-satz.sh`, then
`cargo fmt -p satz-studio-core -p satz-studio -- --check` (the two packages;
`vendor/satz` is satz's own), `cargo clippy --workspace --all-targets --locked -- -D
warnings`, `cargo test --workspace --locked` and `cargo build -p satz-studio --locked`, then
`cargo fetch --locked` and `scripts/update-third-party-licenses.sh --check` with the
cargo-about version the script names, which fails when `THIRD-PARTY-LICENSES.md` is not
what `Cargo.lock` generates or a dependency's licence is not on `about.toml`'s
allow-list.
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
difference is the architecture, and nothing here is architecture-dependent, the webview
and the satz binary being the platform's rather than the chip's.
`release.yml` does not build it either: macOS is Apple silicon in both, and an Intel Mac
gets no bundle. Linux and Windows are unchanged and x86_64 — this is about the Mac. `.github/workflows/names-gate.yml` runs `scripts/check-names.sh` over
the tree and over the commits a pull request adds, or the push to `main` that merges it.

## 6. Decisions

| record | decision |
|---|---|
| [0001](adr/0001-dioxus-desktop-on-the-webview-renderer.md) | Dioxus 0.7 desktop on the webview renderer |
| [0002](adr/0002-a-separate-repository-with-satz-pinned-once.md) | a separate repository; satz pinned once, as the submodule; the binary required at `MIN_SATZ` |
| [0003](adr/0003-the-document-layer-is-the-tree-sitter-grammar.md) | the document layer is the tree-sitter grammar, vendored and compiled in; satz-core stays the authority on meaning |
| [0004](adr/0004-claude-natively-other-providers-adapt-into-its-message-model.md) | Claude natively: the Messages API wire types are the app's message model; other providers adapt into it — superseded by 0020 |
| [0005](adr/0005-tool-approval-by-mcp-annotation-and-the-capability-ceiling.md) | tool approval by the MCP annotations satz declares; the capability ceiling stays satz's — amended by 0020 |
| [0006](adr/0006-apply-and-bootstrap-run-in-the-users-terminal.md) | `apply` and `bootstrap` run in the user's terminal, never with `-auto-approve` |
| [0007](adr/0007-pack-rows-are-derived-from-the-estate-file.md) | superseded by 0018 — pack rows derived from the estate file, the questions report and the resolved params |
| [0008](adr/0008-transcripts-live-outside-the-estate.md) | transcripts live under the app's data directory, never inside an estate — superseded by 0020 |
| [0009](adr/0009-refusal-fallbacks-are-on-by-default.md) | refusal fallbacks are on by default, off by a Settings switch — superseded by 0020 |
| [0010](adr/0010-claude-code-as-the-subscription-backend.md) | Claude Code as the subscription backend: the installed CLI driven over stdio, the estate's satz MCP server, the app's own approval card — superseded by 0020 |
| [0011](adr/0011-the-licence-is-apache-2-0.md) | the licence is Apache 2.0, with `NOTICE` for the material bundled under other terms |
| [0012](adr/0012-migrate-hands-off-to-the-terminal.md) | `migrate` hands off to the terminal with `apply` and `bootstrap`; `bootstrap --dry-run` is a check that runs in the app |
| [0013](adr/0013-the-claude-code-stream-log-is-verbatim-off-by-default-and-bounded.md) | the Claude Code stream log is verbatim, off by default, one file per conversation, and bounded to ten files of 16 MiB — superseded by 0020 |
| [0014](adr/0014-a-newer-satz-is-a-notice-and-the-app-looks-for-releases.md) | a satz newer than the build runs and is a notice, not a gate; the app looks for releases of itself and of satz once per launch and says so in the title and the top bar |
| [0015](adr/0015-the-packs-view-draws-the-dependency-tree-the-packs-declare.md) | superseded by 0018 — the dependency tree the packs declare with `ask_when`, drawn as tree blocks in the grid with connectors in CSS |
| [0016](adr/0016-macos-is-apple-silicon-alone.md) | macOS is Apple silicon alone, in CI and in the release; Linux and Windows stay x86_64 |
| [0017](adr/0017-a-release-is-cargo-release-on-main.md) | a release is `cargo release` on `main`: the version bump is the one commit that lands without a pull request |
| [0018](adr/0018-the-packs-view-shows-satzs-pack-graph.md) | the Packs view shows satz's pack graph (`satz_packs`) and switches a pack with `satz_add_pack` and `satz_remove_pack`; the app derives no pack row and no dependency |
| [0019](adr/0019-the-pack-review-runs-the-cli-and-places-a-private-pack-as-a-local-fork.md) | the pack review runs `satz review-pack` through the CLI with the estate's config; a private pack is placed as `<stem>.local.satz`, never over other text; upstream is a pull request by hand |
| [0020](adr/0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md) | the app runs no model: the Agent destination writes the estate's `satz mcp` configuration for an external client and starts it — superseding 0004, 0008, 0009, 0010 and 0013, and amending 0005 |
| [0021](adr/0021-the-settings-ceiling-is-the-agents-and-studio-writes-at-its-own.md) | the Settings ceiling is the external agent's alone; the app's own `satz mcp` runs at `read,write`; the Agent view shows the ceiling on disk beside the setting and offers Replace where they differ |
| [0022](adr/0022-the-mcp-root-is-the-one-satz-mcp-config-renders.md) | the root of the app's own `satz mcp` is the one `satz mcp-config` renders — the estate's config directory — for the window and every client alike |

## 7. Not built, and why

- **No embedded terminal.** `apply` and `bootstrap` hand stdio to tofu and to the
  human; tofu's approval prompt is the safety step. `EstateSession::external_command`
  writes a one-shot script under `<data dir>/satz-studio/run/` holding `cd '<estate>'
  && '<satz>' --config . <args…>`, and `open_in_terminal` opens it with the OS
  terminal. The app learns nothing from the terminal; `-auto-approve` is never passed.
- **No pack logic of the app's own.** Which packs an estate uses, what each needs and
  what a switch writes are satz's pack graph, read through `satz_packs` and changed
  through `satz_add_pack` and `satz_remove_pack`. The app keeps no table of packs, reads
  no pack file for its dependencies, and writes no pack line itself.
- **No second parser.** The document layer is the tree-sitter grammar and meaning is
  `satz_core::satz::parse`. A grammar gap is fixed in the grammar repository.
- **No agent of the app's own.** satz serves the tools over MCP and the operator's own
  client drives them; the app writes that client's configuration and starts it. No
  model is called, no credential is held, no conversation is kept
  ([ADR 0020](adr/0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md)).
- **No merge.** A file that changed on disk under an edit is refused and reloaded.
