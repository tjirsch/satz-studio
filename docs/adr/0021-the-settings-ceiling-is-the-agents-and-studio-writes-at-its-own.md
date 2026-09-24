# 0021 — the Settings ceiling is the agent's, and satz-studio's own session runs at the ceiling its buttons need

- **Status:** accepted
- **Date:** 2026-09-24
- **Deciders:** the maintainer

## Context

`Settings.mcp_allow` is a capability ceiling — `read`, `read,write` or `read,write,exec`,
the groups `satz mcp --allow` takes. [ADR 0020](0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md)
gave it two jobs: it was the `--allow` of the `satz mcp` child satz-studio starts for
every open estate, and the `--allow` `satz mcp-config` writes into an agent's
configuration. Settings said so in one sentence: "lowering it here lowers both". Three
things made that sentence and the view around it untrue.

- **A saved setting reaches no file.** `satz mcp-config --write` refuses satz's own key
  already there with other arguments and replaces it only with `--force`
  ([satz ADR 0055](https://github.com/tjirsch/satz/blob/main/docs/adr/0055-satz-writes-the-mcp-client-configuration.md)).
  Saving Settings rewrites no client configuration, so an agent configured at
  `read,write` keeps running at `read,write` after the setting is lowered to `read`.
- **The Agent view showed the setting, not the configuration.** Its chip read
  "ceiling: <setting>" whatever the client's file held, which is the one value the
  operator needed to see.
- **The ceiling bounds the satz server, not the agent.** The default client, `claude`,
  has a shell of its own and can run `satz` directly. `--allow` confines the MCP tools;
  it is not a sandbox around the agent.

And the shared value broke the window itself. At `read`, satz-studio's own session
refused its own buttons — Accept, a pack switched on or off, "Write them into the
estate", `merge-presets` — each with satz's "needs 'write'", because the operator had
chosen a ceiling for somebody else.

## Decision

- **`Settings.mcp_allow` is the ceiling of the external agent's configuration and of
  nothing else.** It is the `--allow` of every `satz mcp-config` run the Agent view makes.
- **satz-studio's own `satz mcp` runs at `Allow::STUDIO`, which is `read,write`.** Every
  tool a button calls — `satz_interview`, `satz_add_pack`, `satz_remove_pack`,
  `satz_merge_presets`, `satz_update_prerequisites` — needs `write`, and none runs an
  external program, so none needs `exec`. The window's writes stay bounded by the write
  discipline (`docs/architecture.md` §4b), not by the ceiling.
- **The Agent view shows the ceiling of the configuration on disk beside the setting.**
  Each client card reads the file satz names in its notes (`then: … --write   # writes
  <file>`) for satz's key and compares that entry with the block satz printed for the
  setting: not configured, configured at the ceiling shown, or another entry — another
  ceiling, binary or root — with "Replace it", the `--write --force` run. The lead card
  carries the setting as its chip.
- **Saving Settings rewrites nothing.** A client's file changes when the operator presses
  Configure or Replace on the Agent page, and at no other time.
- **Settings says what the value is:** "The ceiling of the satz MCP server an agent is
  configured with, written into the client's configuration when you press Configure on
  the Agent page. It bounds the satz server only: an agent with a shell can run satz
  commands directly."

## Consequences

- The window works at every setting: an operator who hands the agent `read` still
  answers questions and switches packs in the window.
- What the Agent page says about a client is what the client's file holds, read after
  every run of the card. A file edited by hand is read the next time the card renders.
- The app reads a client's configuration file. It reads the one satz names and satz's
  one key in it, and writes it only through `satz mcp-config`.
- The ceiling in the window is fixed in the code. A studio button that comes to need
  `exec` raises `Allow::STUDIO` in the change that adds it.
- Nothing protects against the agent's own shell. That is the client's permission model,
  and the sentence in Settings says so.

## Pros and cons of the options

### A — the setting is the agent's, the window runs at what it needs, and the Agent view reads the file *(chosen)*

- **Good:** each value means one thing, and the value shown for the agent is the one the
  agent runs at.
- **Good:** the window never refuses its own buttons over a choice made for another
  program.
- **Bad:** one more read of a file satz-studio does not own, keyed on satz's notes for
  where it is.

### B — keep one ceiling for both, and rewrite the client's file on save

- **Good:** "lowering it here lowers both" becomes true.
- **Bad:** a save in Settings silently passes `--force` over a file the operator may have
  shaped by hand, which is what satz's refusal exists to prevent.
- **Bad:** the window still refuses its own buttons at `read`.

### C — keep one ceiling for both, and restart the window's session on save

- **Good:** the setting reaches the window at once.
- **Bad:** it keeps the defect: a ceiling chosen for the agent disables the window's
  writes. And it still says nothing true about the agent's file.

### D — drop the setting and let the operator pass `--allow` in the agent's file by hand

- **Good:** nothing in the app to keep true.
- **Bad:** satz's default is `read`, so an agent configured without a choice cannot
  write, and the operator learns why from a refusal. The setting is where the choice is
  made once.
