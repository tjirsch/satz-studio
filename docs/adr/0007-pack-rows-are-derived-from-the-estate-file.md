# 0007 — pack rows are derived from the estate file, no copied table

- **Status:** superseded by [ADR-0018](0018-the-packs-view-shows-satzs-pack-graph.md)
- **Date:** 2026-09-13
- **Deciders:** the maintainer

## Context

satz's map of pack choices is a pack of questions and a list of lines (satz ADR 0007,
`vendor/satz/docs/adr/`): `presets/estate-map.satz` declares one boolean per optional
pack and the security-group model as a `question oneof`, and the estate carries the
matching `use "…" when <gate>` lines. satz keeps that list once, as `PACK_LINES` in
`vendor/satz/src/template.rs` — path, gate, and the phase comment written above it —
and three writers share it. The interview skeleton writes every line commented, in
the exact shape `pack_line` renders (`// use "<path>" when <gate>`). `uncomment_pack`
in `interview.rs` activates a line when its question is answered yes, matching that
shape and nothing else, and never re-comments one. `report_unadopted_packs` in
`main.rs` reports a gate that is true while its line is still commented ("uncomment
it, or `satz interview` will") or absent ("run `satz merge-presets` to write it"), and
`satz_merge_presets` is what writes the missing line.

The app's Map view shows one row per pack — on, off, or missing — with the question
behind it, its phase and its `oneof` group. The question is where the rows come from.

## Decision

A row is derived from three things the app already holds for an open estate, and from
no table of its own:

1. **the file**: `scan_uses` over the `Cst` yields every `use` line, active or
   commented, with its path, its gate and the contiguous comment block above it, which
   is the row's phase; a commented line counts only in the shape `pack_line` writes
   and `uncomment_pack` matches;
2. **the questions report** (`satz_questions`): the map pack's boolean questions, and
   every option of a `oneof` whose options gate lines;
3. **the resolved params**: the gate's current value.

`PackRow` (`crates/satz-studio-core/src/model/mod.rs`) carries the gate, the path, a
`LineState` — `On`, `Off`, or `Absent` when a map question has no line in the file —
the `Choice` (a boolean with its current value and default, or one option of a `oneof`
group), the question row, the phase and the line. An `Absent` row's action is
`satz_merge_presets`, the remedy satz names. The map pack's own line, which has no
gate, is the first row.

The app never edits a `use` line. A toggle is an answer, written by `satz_interview`;
the file is re-read and the rows rebuilt after every commit, reload and answer.

## Consequences

- The app cannot disagree with the file it shows: a row is the line, the question and
  the value, with nothing in between that could be stale.
- A pack line written by hand in an unusual shape — a trailing comment after the gate,
  `//use` without the space — is not a pack row. `uncomment_pack` would not match it
  either, so the app and satz agree on what a pack line is.
- A line that is active while its gate is false (`uncomment_pack` never re-comments,
  so a `oneof` switch leaves the previous option's line active) is shown as a note on
  the row, never repaired by the app.
- The phase is text from the file. A skeleton's phase comments are satz's; a comment
  someone writes above a line is that line's phase, and a line with no comment above
  it has none.
- A pack the library gains after the skeleton was written has no line until
  `satz_merge_presets` writes it; until then the Map view shows it `Absent`, from the
  questions report alone.

## Pros and cons of the options

### A — copy `PACK_LINES` into the app

- **Good:** the phase and the order are known without reading the file; a row exists
  for a pack the estate has no line for.
- **A line the derivation dropped was a pack nobody saw (2026-09-17).** The rule read
  the file for `use … when <gate>` lines and skipped every `use` without a gate, the map
  aside. satz's CIS GCP Foundation baseline was `use`d with no `when` until satz v0.64.0
  gave it one, so an estate could run thirty organisation policies while this view named
  none of them — and the same held for any hand-written or imported line. An un-gated
  line is a row now (`PackRowKind::Plain`): the same card, the same state badge, and no
  switch, because there is no param to write and the app does not comment or uncomment a
  line the operator wrote by hand. The rule is "one row per `use` line", not "one row per
  choice"; what a question decides is what gets a switch.

- **Bad:** it drifts when the library gains a pack, and the app then shows a list the
  binary it drives does not have. Two places for one list is the cost satz ADR 0007
  accepted for the map and the skeleton — inside one repository, paid with a test that
  keeps them equal. A third place in another repository has no such test, only a
  submodule bump someone remembers.

### B — ask satz for the table over MCP

- **Good:** always the pinned release's list.
- **Bad:** no such tool exists, and the file is the record: satz's own decision is that
  the estate carries the lines, so a reader sees what it uses. A table tool would
  answer what the skeleton would write, not what this estate has.

### C — derive the rows from the file, the questions and the params *(chosen)*

- **Good:** three inputs the app has already; the rows are true of this file; the
  shape rule is shared with `uncomment_pack`, so a line satz can activate is a line
  the app shows.
- **Bad:** the app knows no phase for a line whose comment block is missing, and no
  path for a question whose line is absent — only that it is absent.
