# 0008 — transcripts live outside the estate

- **Status:** superseded by [ADR 0020](0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md) — the app keeps no transcript: there is no conversation of its own to keep
- **Date:** 2026-09-13
- **Deciders:** the maintainer

## Context

A conversation about an estate names it: the directory and project ids the tools
return, the resource addresses the model reasons about, the diagnostics quoted into
the system prompt, the identity the estate runs as. The Chat view keeps that
conversation so it can be resumed — the transcript is the `Vec<Message>` the next
request is built from, thinking blocks and tool results included.

An estate is a git repository, and its history belongs to a customer. satz's privacy
gate (`scripts/check-names.sh`, which this repository carries as a copy) rejects a
commit that stages a local file — `CLAUDE.local.md`, `*.local.md`, `.claude/`,
`attestations.yaml`, `evidence/` — because a file that sits beside an estate is a file
that gets committed. The question is where a transcript is written, and whether at
all.

## Decision

Transcripts live under the app's data directory, never inside an estate
(`crates/satz-studio-core/src/transcript.rs`):

```
<data dir>/satz-studio/transcripts/<sha256 of the estate path>/<created>.jsonl
```

`<data dir>` is the platform's (`dirs::data_dir()`); `<created>` is the RFC 3339
instant the transcript was started, with colons replaced. Line 1 is the header — the
estate path, the model, the instant — and every other line is one `Message`, appended
and flushed as the turn produces it; a file that exists is never overwritten, and a
line that is neither a header nor a message fails the load. `Settings.persist_transcripts`
(on by default) switches persistence off. A model switch starts a new transcript,
since a thinking block is echoed on the model that signed it.

The app writes no credential and no transcript into an estate. The credential is the
environment's, the `ant` profile's or the OS keychain's (`llm/auth.rs`); the one-shot
command scripts (ADR 0006) live under the same data directory.

## Consequences

- Transcripts do not travel with the estate repository. A clone on another machine
  starts with none; a conversation is local to the machine that held it.
- The directory key is the estate path as opened. A moved or renamed estate starts a
  new transcript directory; the old one stays under the old hash, its header naming
  the old path. The hash is a filesystem-safe key, not a disguise: the header carries
  the path in clear, and the messages carry what the tools returned.
- The data directory is as private as the user account that owns it. Nothing in it is
  encrypted by the app.
- The privacy gate has nothing new to catch: no file the app writes can be staged in
  an estate.

## Pros and cons of the options

### A — the app's data directory, keyed by the estate path *(chosen)*

- **Good:** outside every repository by construction; one place to find, back up or
  delete every conversation; the gate needs no new rule.
- **Bad:** a transcript and its estate can part ways — a moved estate, a second
  machine — and the app has no way to reunite them.

### B — a `.satz-studio/` directory inside the estate

- **Good:** the transcript travels with the estate; a colleague opening the same
  checkout sees the same conversations.
- **Bad:** gitignored until someone forgets, or until a checkout without the ignore
  rule stages it; the gate has no rule for the directory, and a rule would have to
  reach every estate's copy of the gate. A conversation that names project ids would
  then be in a customer's history, and a history is not undone by a later commit.

### C — no persistence

- **Good:** nothing to protect; the conversation ends with the window.
- **Bad:** a conversation cannot be resumed after the app closes or the estate is
  reopened: the tool calls, their results and the model's reasoning about this estate
  are gone, and the next session starts from the system prompt alone. Resuming needs
  the messages the API saw, and only a stored transcript has them.
