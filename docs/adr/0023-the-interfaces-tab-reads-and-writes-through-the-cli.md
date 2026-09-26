# 0023 — the Interfaces tab reads and writes through the CLI

- **Status:** accepted
- **Date:** 2026-09-26
- **Deciders:** the maintainer

## Context

A central estate publishes to the projects that read it: `export` lines and
`interface "<name>" { … }` blocks, which `satz transpile` writes to `interfaces/<name>/`
(satz's [ADR 0070](../../vendor/satz/docs/adr/0070-an-estate-publishes-a-declared-interface-module.md)).
satz 0.86.1 reports what an estate publishes with `satz interfaces <estate> --format
json` — every export with its interface, how a project holds it, its value, its attach
points and where it is declared, and every interface with what it uses — and writes a
new one with `satz add-project`: a project's whole section (a Google project under the
workload folder, its IaC service account, its state bucket, their grants and its
interface) or, with `--interface-only`, the interface block alone. A refusal exits
non-zero with satz's sentence on stderr and writes nothing.

Neither command has an MCP tool. The maintainer paused the interface plane's MCP tools,
and the app's own `satz mcp` is the same server an external agent is configured with
(ADR 0020, ADR 0021): a tool the app asks satz to serve is a tool every agent is served.

The write is to the estate's main file, which the app writes only under the discipline of
`docs/architecture.md` §4b: satz's own writer on the real file, the bytes recorded first,
the check afterwards, the recorded bytes back on a refusal.

## Options

1. **MCP tools (`satz_interfaces`, `satz_add_project`).** One transport, and the write
   is a delegated write exactly like `satz_add_pack`. Costs unpausing the interface
   plane's MCP tools in satz, which is satz's decision and is not taken: every agent
   would gain them with the window.
2. **The app's own writer: a new `Edit` kind that appends the section.** The document
   layer's discipline (render, splice, prove, temp file, check, rename). Costs a second
   writer of a section satz already writes — the text of a project's section, the copied
   export lines, the refusals (a name declared twice, no `workload_folder`, a core export)
   would be the app's copy of satz's, and drift from it on the next satz release.
3. **The CLI, inside the delegated-write discipline** *(chosen)*. The read is
   `SatzCli::json_report` over `satz interfaces`; the write is `satz add-project` run by
   the session's `SatzCli` as the call inside `Snapshot::delegate`, its process result
   mapped to a `ToolOutcome` — a non-zero exit is `is_error` with satz's stderr sentence,
   a zero exit the line satz ends on. It is satz's own writer on the real file, so it is
   no second way to write a file. Costs a CLI call per reload (`satz interfaces`
   compiles the estate) and a second transport for one write, which the MCP tools
   replace when they arrive.

## Decision

**Option 3.** `satz_studio_core::satz::project` holds `interfaces` (the read),
`AddProjectArgs` with `argv` and `problem` — satz's own name, owner-group and
interface-only rules, so the form says them before anything runs — and `add_project`,
the call. `edit::delegated_write` is the write lock, the snapshot and
`Snapshot::delegate` with `McpChecker`, the one path every delegated write of the app
takes, the tools' and this command's alike; the e2e tests drive it.

The Interfaces view is a third tab of the Estate destination, not a rail destination:
the rail holds six primary destinations, the navigation-rail pattern's limit in this
app (`docs/ui.md`), and what an estate publishes is part of what the estate file says.

## Consequences

- Every reload runs `satz interfaces` beside `satz_questions` and `satz_packs`. A
  failure is satz's reason in the tab and a diagnostic in the drawer, never an estate
  that publishes nothing.
- `MIN_SATZ` is 0.86.1: the app uses a command that release introduced.
- When satz serves the interface plane over MCP, `add_project` is replaced by
  `session.tool("satz_add_project", …)` inside the same `delegated_write`, and the read
  by the tool; this record is then superseded in that part.
