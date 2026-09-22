# 0020 — the agent is an external client that satz-studio configures and starts

- **Status:** accepted
- **Date:** 2026-09-22
- **Deciders:** the maintainer

## Context

satz serves its own tools over the Model Context Protocol: `satz mcp --root <dir>
--allow <ceiling>` is a stdio server with twenty-two tools, an instruction block and a
working guide, and satz's capability ceiling is enforced inside it. Anything that speaks
MCP can drive an estate through it.

satz-studio carried a second client for that one server. The Chat destination ran an
agent loop of its own over two engines — the Messages API (`src/llm/claude/`,
`src/llm/agent/`, `src/llm/provider/`) and the installed Claude Code CLI driven over
stdio (`src/llm/claude_code/`) — with its own approval card, its own transcripts, its own
debug log and its own settings. What that cost, counted in this repository before the
change:

| part | lines |
|---|---|
| `crates/satz-studio-core/src/llm/` | 5,177 |
| `crates/satz-studio/src/views/chat/` | 3,824 |
| `crates/satz-studio-core/tests/llm_*.rs`, `tests/claude_code_*.rs` | 3,890 |
| `crates/satz-studio-core/tests/fixtures/{llm,sse,claude_code}/` | 820 |
| `crates/satz-studio/assets/css/chat.css` | 617 |
| `crates/satz-studio-core/src/transcript.rs` | 247 |
| **total** | **14,575** |

against 47,075 lines of Rust in the two crates: not quite a third of the app, for the
half of the work the app is worst placed to do.

The dependencies are the same story. The chat is the only thing in the repository that
holds a credential, and therefore the only reason five crates are compiled in —
`keyring`, `keyring-core` and the three platform stores (`apple-native-keyring-store`,
`windows-native-keyring-store`, `zbus-secret-service-keyring-store`) — and the only
reason `reqwest` streams a response body. Everything else the app does over the network
is two short GitHub reads.

And it is maintenance against a moving target. Both engines are written against clients
that gain features every few weeks: the Messages API's blocks, betas and stop reasons on
one side, Claude Code's control protocol, permission modes and stream shapes on the
other. Each addition is a shape the app must learn or carry verbatim, and a client that
falls behind is a client that shows the model's work wrong. Meanwhile the operator
already has agents that speak MCP and stay current without this repository's help:
Claude Code, cowork, Claude Desktop.

What the app is uniquely good at is the other half: the estate as a picture. The
interview, the pack graph, the typed fields over a file whose comments and columns
survive an edit, the review, the diagnostics at their lines, the decisions sheet. None
of that exists anywhere else.

## Decision

**satz-studio runs no model.** The Chat destination, both engines, the transcripts, the
credential handling and the engine settings are deleted. In the rail's second secondary
slot stands **Agent**, which sets an external agent up on the open estate and starts it:

- `crates/satz-studio-core/src/handoff.rs` renders the `satz mcp` invocation for the
  open estate in the two shapes a client reads — `.mcp.json`, the project file Claude
  Code reads in the directory it starts in, and the `mcpServers` block for Claude
  Desktop's configuration file, keyed `satz-<estate>` because one file there holds every
  server. Both carry the satz binary the app located, the session root
  `satz::session::session_root` computed for this estate, and `--allow` with the ceiling
  from Settings; satz's own default is `read`, so a configuration that left it out would
  hand the agent a read-only server without saying so.
- The view writes `.mcp.json` into the estate's directory, refusing a file of that name
  that holds anything else until the operator asks for it to be replaced; copies either
  shape; and starts the configured agent command in the estate's directory.
- Settings carries one new field, `agent_command`, defaulting to `claude`. The engine
  fields — the provider, the model, the effort, the fallbacks, the transcripts switch,
  the chat debug log, the Claude Code binary and its stream log, and the auto-approval of
  write tools — are gone with the code that read them.

**The exception is option C, narrowly.** A view that genuinely needs a model — a
sentence about a finding, a value proposed from a prompt — may call one from that view,
with no chat window, no loop and no tool use, and it gets its own record. Nothing does
today.

This supersedes [0004](0004-claude-natively-other-providers-adapt-into-its-message-model.md),
[0008](0008-transcripts-live-outside-the-estate.md),
[0009](0009-refusal-fallbacks-are-on-by-default.md),
[0010](0010-claude-code-as-the-subscription-backend.md) and
[0013](0013-the-claude-code-stream-log-is-verbatim-off-by-default-and-bounded.md), and
amends [0005](0005-tool-approval-by-mcp-annotation-and-the-capability-ceiling.md): its
approval card is gone, and its other half stands — the capability ceiling is satz's, and
`--allow` is what the handoff writes into the agent's configuration.

## Consequences

- **No credential of any kind is in the app.** No API key, no keychain entry, no
  keyring crate, no `ANTHROPIC_*` read. The one credential satz-studio has ever needed
  stays gcloud's, and satz reads it.
- **No transcript.** Nothing the app keeps names a project, a resource or an
  organisation; the conversation belongs to the client that held it.
- **satz-studio cannot answer a question about an estate.** "Which questions are open?"
  is a view, not an answer: Decisions. An operator who wants a sentence back asks the
  agent, in the agent's own window.
- **The agent runs outside the window, so the app does not see its writes.** The estate
  is re-read when the window regains focus — the mechanism `apply` and `bootstrap`
  already rely on ([ADR 0006](0006-apply-and-bootstrap-run-in-the-users-terminal.md)) —
  and the top bar's reload is always there.
- **A customer who will not install a terminal client needs the Desktop path.** That is
  why the second shape exists and why the view spells out where the block goes: Claude
  Desktop is configured once, from a block on the clipboard, and needs no command line.
- **One MCP ceiling, stated in two places.** The app's own `satz mcp` child and the
  agent's are both started at `Settings.mcp_allow`. An operator who lowers it lowers both.
- **The agent starts in the OS terminal**, through the one-shot script `apply` and
  `bootstrap` use, because the default client is a terminal program and a GUI process
  spawned without one shows nothing. The command is resolved on `PATH` first, so a
  command that is not installed is named as missing rather than flashing a terminal.

## Pros and cons of the options

### A — hand off to an external agent and start it from the window *(chosen)*

- **Good:** the MCP server already exists and is satz's; the clients already exist and
  are maintained by the people who ship them. The app keeps no credential and no
  conversation. Fourteen and a half thousand lines and six crates leave the tree. The
  estate the agent is pointed at is the one on screen, with the ceiling the operator set.
- **Good:** the two entry points the decision names are both served — `.mcp.json` for a
  terminal client started from the window after the mechanics are done, and a Desktop
  block for an operator who starts from the agent.
- **Bad:** two windows. The operator moves between the picture of the estate and the
  agent talking about it, and nothing correlates them.
- **Bad:** satz-studio cannot answer anything itself, so a first-time operator who opens
  the app expecting to ask it something has to be told where to ask.
- **Bad:** the app cannot show what the agent did while it did it. It sees the file
  afterwards, on focus or on reload.

### B — keep the chat and maintain a second MCP client

- **Good:** one window for the whole job; the approval card in the app's own words,
  beside the estate it is about; the tool call, its result and the diagnostics it raised
  in one place.
- **Bad:** a third of the app, and the only credential handling in it, maintained against
  two clients that change every few weeks — for a capability the operator can already get
  from a client that is better at it.
- **Bad:** the app is behind by construction. A block type, a stop reason or a control
  request that is added upstream is either learnt here or carried verbatim, and until it
  is learnt the window shows the model's work incompletely.
- **Bad:** it puts satz-studio in the business of being an agent client, which is not
  what it is for and not where its advantage is.

### C — a narrow model call inside a single view, with no chat window

- **Good:** the small thing a model is genuinely better at — explaining a finding,
  proposing a value — without a loop, tools, approval or a transcript.
- **Bad:** it brings the credential back, and with it the keychain and the five crates,
  for one sentence.
- **Bad:** it is the chat again in eighteen months unless the rule is written down: no
  loop, no tools, no window.
- **Kept as the exception**, not the decision: a view that needs one gets its own record
  saying what it calls and why a view cannot do the job.

### D — no agent story at all, and let the operator configure MCP by hand

- **Good:** nothing to build and nothing to maintain; the satz documentation already
  says what `satz mcp` is.
- **Bad:** the invocation is not obvious — the root is a computed boundary, not the
  estate directory, and the ceiling defaults to `read` — so an operator writing it by
  hand gets a server confined to the wrong place or unable to write, and blames satz.
- **Bad:** the app already knows every value in it, and it is the app that knows which
  estate is open.
