# 0005 — tool approval by MCP annotation, and the capability ceiling

- **Status:** accepted; the approval card is gone with the chat ([ADR 0020](0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md)), and the capability ceiling stands — `--allow` is what satz-studio writes into an agent's own MCP configuration
- **Date:** 2026-09-13
- **Deciders:** the maintainer

## Context

The agent runs the satz tools through the app: a `tool_use` block in the model's
message becomes a call on the estate's `satz mcp` child (`EstateSession::tool`), and
the result goes back as a `tool_result`. Some tools only compile and report; some
write the estate (`satz_interview` with answers, `satz_transpile`, `satz_adopt` with
`execute`, `satz_merge_presets`); one overwrites packs the estate uses
(`satz_get_presets` with `force`); one runs an external scanner (`satz_scan_checkov`).

satz decides two things about every tool, and the app has both at hand. The server
annotates each tool at `tools/list` — `readOnlyHint`, `destructiveHint`,
`idempotentHint`, `openWorldHint`, the table in `vendor/satz/docs/mcp.md` — and
`McpSession` keeps them as `ToolAnnotations` on `ToolInfo`. And `satz mcp --allow`
confines the server to a ceiling of capability groups (`read`, `write`, `exec`) that
the client cannot raise; a call above it returns a result with `isError`, not a
protocol error. The question is what the app asks the operator before a call, and
what it decides on its own.

## Decision

Approval is a property of the tool as satz declares it. In `Agent::execute_tools`
(`crates/satz-studio-core/src/llm/agent/mod.rs`):

- a tool with `read_only_hint == true` runs without asking;
- every other call raises `AgentEvent::ToolCallPending` with the tool's name and input
  and waits for the operator's `Approval`: **Once** runs this call; **ForSession**
  runs it and remembers the tool for the rest of the session; **Deny** returns a
  `tool_result` with `is_error: true` saying the operator denied it, so the model
  continues and can ask or do something else;
- a tool with `destructive_hint == true` asks every time; "for the session" does not
  cover it;
- the per-estate setting `auto_approve_writes` (`Settings`, off by default) skips the
  card for a tool that is neither read-only nor destructive.

A tool the server does not list, or a call whose input is not a JSON object, is
refused the same way — an error result, never a protocol error. A tool that is not
read-only runs under the estate's write lock, like every other writer.

The ceiling stays satz's. `Settings.mcp_allow` (`Allow::ReadWrite` by default) is the
value every `satz mcp` this app starts is given; `exec` is what lets
`satz_scan_checkov` run Checkov. The app never raises it after the child is started,
and a call the ceiling refuses is shown as satz's own refusal.

## Consequences

- A new satz tool is gated correctly on day one: the app reads what the release
  declares, and there is no list in the app to update.
- A tool without annotations asks. `ToolAnnotations` defaults every hint to `None`,
  and only `Some(true)` on `read_only` runs without a card.
- Read-only is about the estate, not the network: `satz_report_compliance` and
  `satz_whoami` reach the live organisation (`openWorldHint`) and still run without a
  card, since they write nothing. What bounds them is the identity satz binds per
  estate, which the app displays and cannot change.
- `exec` is a Settings decision, taken once: without it satz refuses `satz_scan_checkov`
  whatever the operator approves. With it the call is approved like any tool that is
  not read-only — a card, unless it was approved for the session or
  `auto_approve_writes` is on — because satz annotates it so: it runs an external
  program, Checkov, which `uvx` downloads first when it is not on the `PATH`. The
  remediation tools read the report a scan wrote and run nothing:
  `satz_remediation_items` is read-only and needs no `exec`.
- The operator sees every write before it happens, and the transcript carries a
  denial as the model saw it.

## Pros and cons of the options

### A — approve by the tool's annotations, the ceiling from Settings *(chosen)*

- **Good:** one source for what a tool does, kept by the project that writes the tool;
  the annotations and the ceiling are the two halves satz's own documentation
  describes, and the app implements them as written.
- **Bad:** the app trusts the hints. A tool annotated read-only that writes would run
  unseen; the guard against that is satz's smoke matrix, not the app.

### B — approve everything

- **Good:** no cards; the agent works at its own pace.
- **Bad:** an agent writes the estate unseen. The write discipline checks every file
  it writes, but a check says the file compiles, not that the operator wanted it.

### C — a hand-kept list of tool names in the app

- **Good:** the app decides per tool, with judgement the hints cannot carry.
- **Bad:** it drifts from satz's own table the first time a tool is added or its
  behaviour changes, and a tool the list does not name falls to a default that is
  wrong one way or the other.

### D — approve every call

- **Good:** nothing runs unseen.
- **Bad:** unusable. A turn that reads the questions, checks the transpile and opens a
  report is three cards before anything happens, and the operator learns to click
  through — which is the failure B has, with more clicks.
