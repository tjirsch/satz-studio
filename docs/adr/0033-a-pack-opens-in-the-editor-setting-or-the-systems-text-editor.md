# 0033 — a pack opens in the Editor setting, or the system's text editor

- **Status:** accepted
- **Date:** 2026-11-20
- **Deciders:** the maintainer

## Context

[ADR 0031](0031-the-pack-review-is-in-the-palette-and-a-new-pack-opens-in-the-systems-editor.md)
opened a pack's file — from Edit and from New pack — in the application the system gives
`.satz` files, through the `open` crate, with no setting. On macOS that is `open <file>`,
and with no application registered for `.satz` it exits 1 ("No application knows how to
open"): the pencil did nothing but raise a toast quoting the launcher's exit status, and
the path in it still carried the estate's `../..`. No system registers an application for
`.satz` out of the box, so the failure was the common case, not the exception.

## Considered options

1. **An Editor setting, empty by default; empty means the system's text editor** —
   `open -t` on macOS, `xdg-open` on Linux, `notepad` on Windows. A set command line is
   split into words (a double-quoted word kept whole) and run with the file as its last
   argument. The app does not wait on the editor's window; a command that cannot start
   or exits with a failure is a toast naming the command and Settings → Editor. Saving a
   command whose program is not on `PATH` and is not a program at the path given is
   refused, with the reason under the field. Cost: one more setting, and `xdg-open` on
   Linux is still the MIME handler the desktop picks for the file.
2. Keep the registered application and explain the failure better. Cost: the pencil keeps
   failing on every fresh system until the operator registers an editor outside the app.
3. A setting with no default — refuse Edit until one is set. Cost: a first press that does
   nothing useful, when every system has a text editor.

## Decision

Option 1. The path is resolved by its text (`.` and `..` removed) and checked to be there
before anything starts. Show folder and the other folder and export opens stay on the
`open` crate: a folder, a Markdown sheet and a workbook have a registered application.
This replaces 0031's answer to "what opens a pack's file"; the rest of 0031 stands.

## Consequences

- The settings file carries `editor`, a string; a file without the key reads it as empty,
  like every other key of the settings file, and a file that does not parse stays a
  refusal.
- `satz_studio_core::editor` holds the command line, the platform's text editor, the
  check and the start; the window's `edit_file` runs it on a task of its own.
