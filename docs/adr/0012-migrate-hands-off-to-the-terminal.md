# 0012 — `migrate` hands off to the terminal, with `apply` and `bootstrap`

- **Status:** accepted
- **Date:** 2026-09-16
- **Deciders:** the maintainer

## Context

The Deploy destination gathers the commands that hand an estate off: `hcl-init`, `plan`,
`apply`, `migrate`. [ADR 0006](0006-apply-and-bootstrap-run-in-the-users-terminal.md)
sent `apply` and `bootstrap` to the user's own terminal, and its reason for `apply` was
the tool's approval prompt: the app has no way to answer it and no business answering it.

`satz migrate <estate> --mode <local|cloud>` does not have that prompt. It rewrites
`deployment_mode` in the estate file, runs `transpile` again, and then runs the
configured tool's `init -migrate-state -force-copy` in `hcl_dir` — `-force-copy` is
there precisely so that nothing asks. So the ADR 0006 criterion, read literally, does
not reach it: the command is non-interactive and the app could stream it like any other.

Two other rules do reach it. The app writes an estate file in one way only
(`docs/architecture.md` §4b): under the session's write lock, checked by
`satz transpile --check`, rolled back to the recorded bytes if the check refuses.
`migrate` writes the estate with satz's own writer, outside that discipline, while the
window holds a model of the file it is rewriting. And the second half of the command
copies live state between a local file and a bucket, which is not undone by restoring
the file the first half wrote.

## Decision

`migrate` is `external: true` in the palette (`crates/satz-studio/src/views/commands.rs`),
beside `apply` and `bootstrap`: the app shows the command line, copies it, or opens it
in the OS terminal through `EstateSession::external_command`. The estate is reloaded when
the window regains focus, as it is after an apply.

`bootstrap --dry-run` is a separate palette entry, `bootstrap-check`, and it runs IN the
app: it creates nothing, writes nothing and prints the plan and the permission pre-flight,
so it is a check, and the Overview offers it as one.

## Consequences

- Three commands leave the window instead of two, and the rule behind the list is
  stated once: a command that changes a live organisation, or rewrites the estate
  outside the app's write discipline, is a hand-off.
- The two halves of day 0 are one click apart and clearly different: checking it is in
  the app, doing it is in the terminal.
- A state migration cannot be started by a misclick in a palette, and cannot half-happen
  under a window that is showing the file it rewrote.
- The app learns nothing from the run, as with `apply`; what it knows is the estate on
  the next reload and `hcl_dir` as the tool left it.

## Pros and cons of the options

### A — a terminal hand-off, like `apply` and `bootstrap` *(chosen)*

- **Good:** one rule for every command that changes a live organisation; the estate is
  never rewritten by a command running under the window's own model of it; the state
  copy happens where the operator sees every line of it.
- **Bad:** a second window for a command that does not technically need one.

### B — run it in the app like any other command

- **Good:** one window; the log beside the estate it changes.
- **Bad:** the estate file is rewritten outside the write lock and the check, so the
  window's model and the file disagree until the next reload, and a refusal has nothing
  to roll back to; and a live state copy starts from a button.

### C — run it in the app under the write lock, with a snapshot and a check

- **Good:** the estate half obeys the write discipline: recorded bytes, the check on the
  real path, a rollback if it refuses.
- **Bad:** the rollback is a lie. Restoring the estate file does not move the state back,
  so a refusal after the copy leaves `deployment_mode` saying one thing and the state
  living somewhere else — worse than not offering the command in the app at all.

### D — not offering `migrate` at all

- **Good:** nothing to decide; the operator types one command.
- **Bad:** Deploy is where the estate is handed off, and switching an estate between
  local and cloud state is part of that. Showing the command line and opening it is what
  the app does for the other two.
