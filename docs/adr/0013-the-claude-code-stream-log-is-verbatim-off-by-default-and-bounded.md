# 0013 — the Claude Code stream log is verbatim, off by default, and bounded

- **Status:** superseded by [ADR 0020](0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md) — there is no Claude Code session of the app's to log
- **Date:** 2026-09-16
- **Deciders:** the maintainer

## Context

A turn on the Claude Code engine ([ADR 0010](0010-claude-code-as-the-subscription-backend.md))
is a stream of JSON lines the CLI writes and the app folds. When the fold fails, the
operator sees one line in the chat — the error — and nothing of the stream that
produced it. A stream error reported that way could not be answered with "what did the
model actually send": the lines were read, parsed and gone. The case that made this
concrete was a tool called without arguments, whose input arrived as an empty fragment
and failed a parse; the toast said so, and nothing said what the fragments were.

What the stream carries is not neutral. The tool results are the estate's contents and
resource names; the user messages are what the operator typed; the command line holds
the estate path and the system prompt. [ADR 0008](0008-transcripts-live-outside-the-estate.md)
already decided where such material lives — under the app's data directory, never inside
an estate — and this record decides what is written there for the stream, when, and how
much.

## Decision

`llm::claude_code::log` writes a session's stream log when `Settings.claude_code_log`
is on, and nothing when it is off, which is the default.

- **Verbatim, before parsing.** Every line the CLI writes on stdout is recorded as it
  arrived, before the parser reads it; every line the app writes on stdin, and every
  line of the CLI's stderr, is recorded too, each marked by its channel; the app adds
  a header naming the session and, when a turn ends in an error, the error it made of
  the lines above. Nothing is redacted.
- **One file per conversation**, which on this engine is one process:
  `<data dir>/satz-studio/logs/claude-code/<created>.log`, one record per line,
  `<instant>\t<channel>\t<line>`.
- **Bounded.** A file stops recording at 16 MiB and says so in its last line; opening
  a session keeps the ten newest logs and deletes the rest. The whole directory is at
  most 160 MiB.
- **Local.** The log is not uploaded, not attached to anything, and never a fixture.
  The Settings card says in one sentence what it contains and offers "Reveal logs",
  which opens the directory in the OS file manager.

## Consequences

- A stream error is diagnosable after the fact: the stdout records replay into the fake
  CLI's recorded-stream format, and the app's own note sits after the line it refused.
- Turning the log on is a decision to keep estate contents and typed text on disk in
  clear, for as long as the ten newest sessions last. The data directory is as private
  as the account that owns it (ADR 0008); nothing here encrypts it.
- A session that writes past 16 MiB loses its tail from the log. "New" starts a new
  process and a new file.
- A log that cannot be written fails the turn, naming the file: an operator who turned
  the log on is told it is not recording rather than finding an empty file later.
- A change of the switch applies from the next session, because the log is opened when
  the process starts.

## Pros and cons of the options

### A — verbatim, off by default, bounded by size and count *(chosen)*

- **Good:** the log is exactly what arrived, so a parser bug cannot hide in it; nothing
  is kept unless the operator asked; the disk use has a ceiling.
- **Bad:** what is kept is everything, estate contents included; a very long session is
  cut at the bound, and the part that is cut is the latest.

### B — redacted on the way in

- **Good:** less sensitive material on disk.
- **Bad:** a redaction rule runs on the same bytes the parser runs on, so the log would
  show a transformed stream — the thing a stream log exists to avoid. What counts as
  sensitive is the estate's names and values, which have no shape a rule can find; a
  redacted log would still hold most of it while no longer being a faithful record.

### C — always on

- **Good:** the first failure is already recorded; nobody has to reproduce it.
- **Bad:** every conversation's contents stay on disk whether the operator wants that or
  not, which [ADR 0008](0008-transcripts-live-outside-the-estate.md) gives a switch for
  even for transcripts. The Claude Code engine keeps no transcript at all; an always-on
  log would be one, unasked.

### D — one rolling file for all sessions

- **Good:** a single bound, one file to open.
- **Bad:** sessions interleave when two estates are open, and deleting one conversation
  means editing a file rather than removing it. A file per conversation is what the
  operator can hand to a bug report or delete.
