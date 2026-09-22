# 0010 — Claude Code as the subscription backend

- **Status:** superseded by [ADR 0020](0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md) — Claude Code is the agent, started from the window rather than driven by it
- **Date:** 2026-09-15
- **Deciders:** the maintainer

## Context

The Chat view drives satz through a model over the Messages API, with an API key
([ADR 0004](0004-claude-natively-other-providers-adapt-into-its-message-model.md)).
Many of the people who would use this app pay for a Claude Max subscription and have
the Claude Code CLI installed and signed in; they have no API key, and a key would be a
second bill for the same work. A subscription is used programmatically through Claude
Code, not through the Messages API.

The OAuth token behind that subscription is Claude Code's. It sits in the CLI's own
store, it is refreshed by the CLI, and the terms it is issued under are Claude Code's.
An app that reads it and calls the API with it is using a credential that was not
issued to it.

Claude Code has a headless mode that is exactly a backend: `claude -p --input-format
stream-json --output-format stream-json` reads one JSON object per line on stdin and
writes one per line on stdout, and its `stream_event` lines carry the Messages API's own
stream events. Over the same pipe runs a control protocol: the client sends an
initialize request, and the CLI sends back a `can_use_tool` request whenever a tool
needs permission — so the app can keep its own approval card (ADR 0005) rather than
handing permission to Claude Code's settings.

ADR 0004 kept this as "a second backend behind `ChatProvider`". That is not where it
fits: `ChatProvider` streams one request and the app's `Agent` owns the loop and the
tool calls, while Claude Code owns both. The choice is therefore not which provider the
agent uses but which engine serves the view.

## Decision

A second engine, chosen in Settings as `ProviderChoice::ClaudeCode`.
`llm/claude_code/` drives the installed CLI over stdio; the Chat coroutine holds either
`Engine::Api(Agent)` or `Engine::ClaudeCode(Session)`, and both raise the same
`AgentEvent`s, so the view, the approval card and the tool cards are one implementation.

- **The app reads no credential of Claude Code's.** `claude auth status --json` is all
  it asks: whether the CLI is signed in, through what, and to which address. Signing in
  and out are `claude auth login` / `logout` in the user's own terminal — the same
  one-shot script mechanism `apply` and `bootstrap` use (ADR 0006), because the login
  opens a browser. `AuthStatus`'s `Debug` redacts the address, so it reaches the
  Settings card and no log line.
- **The session is the app's, not the user's Claude Code setup.** `--setting-sources ""`
  leaves their global settings out, `--strict-mcp-config` their MCP servers,
  `--tools ""` every built-in tool. What the model can call is one MCP server: this
  estate's own `satz mcp`, under the ceiling `Settings.mcp_allow` names, with its tools
  as `mcp__satz__…`. `--bare` is never passed: bare mode never reads the subscription
  login, which is the whole point.
- **Approval stays the app's.** `--permission-prompt-tool stdio` routes a tool that is
  not pre-approved to the client as a `can_use_tool` request; the app raises its
  approval card and answers `allow` or `deny`. The `--allowedTools` list is derived from
  the annotations satz declares, exactly as ADR 0005 decides: read-only tools run
  without a card, and the non-destructive writes join them when
  `auto_approve_writes` is on. A destructive tool is never pre-approved.
- **The estate's write lock is held for the whole turn**, not per call: Claude Code's
  satz server writes the estate, and the app cannot see the calls it pre-approved.
- **The conversation is Claude Code's.** The app writes no transcript for this engine
  (ADR 0008 governs the other one); "New" starts a fresh process, and the model is a
  Settings decision because it is a command-line argument of that process.

## Consequences

- The loop, the context window, the compaction and the effort are Claude Code's. The
  composer shows no effort control and no thinking or caching chips on this engine, and
  says that tools run inside Claude Code.
- Usage comes from the assistant messages and the `result` line, and the plan's own
  windows arrive as `rate_limit_event`. That becomes `AgentEvent::Notice`, which the
  footer shows: how much of the five-hour or seven-day limit is used and when it resets.
- The estate's `CLAUDE.md` loads, because the process runs with the estate directory as
  its working directory. That is wanted — it is the estate's own working rules — and it
  is context the API engine does not have.
- Claude Code's satz MCP server is a second `satz mcp` process, separate from the one
  `EstateSession` holds, and no estate is open in it. The appended system prompt names
  the estate and its config and tells the model to call `satz_open` first.
- `system/init` arrives with the first turn, not with the initialize answer, so the
  check that the satz server connected happens as the first turn starts. A session
  whose satz server is neither connected nor pending fails the turn naming the status.
- A version of Claude Code that changes the control protocol or the shape of these lines
  breaks this engine. The lines are typed (`llm/claude_code/events.rs`) and a line that
  is not JSON at all is an error; the offline tests replay recorded streams, so a change
  arrives as a test that fails rather than as a wrong answer.
- Two engines mean two paths through the Chat coroutine. They share Send, Cancel,
  Approve and the whole view; what differs is the transcript and the model switch.

## Pros and cons of the options

### A — drive `claude -p` over stdio with the control protocol *(chosen)*

- **Good:** the subscription is used the way it is meant to be, with no credential in
  the app; the tools are still the estate's, the approval card is still the app's and
  the write lock is still held; the stream is the Messages API's own, so the existing
  assembler and the existing view serve both engines.
- **Bad:** the app depends on a CLI's flags and its control protocol, neither of which
  is a stable published interface; the context window and the loop are outside the
  app's sight, so "what did it send" has no answer the app can give.

### B — read Claude Code's stored credential and call the API with it

- **Good:** one engine, one loop, one code path; everything the app already does keeps
  working unchanged.
- **Bad:** the token was issued to Claude Code, not to this app — using it is not
  permitted, whatever the file permissions allow. It is also brittle: a refresh, a
  re-login or a change of store format breaks it silently, and the failure looks like
  an API error rather than a wrong design.

### C — `--permission-prompt-tool` pointing at an MCP server of the app's own

- **Good:** the permission hand-off is an MCP tool call, a documented shape, rather than
  a control request on the same pipe.
- **Bad:** a second process and a socket between them, for one round trip per tool call,
  and the app would have to route each answer back to the right Chat view. The control
  protocol carries the same request on the pipe that is already open.

### D — a read-only Claude Code engine, with no approvals at all

- **Good:** nothing to answer: pre-approve the read-only tools and let anything else be
  denied by the CLI.
- **Bad:** every write is denied, so the engine cannot answer a question, adopt an id or
  run the interview — the work the app exists for. The operator would switch to the API
  engine to do anything, which is the engine they have no key for.
