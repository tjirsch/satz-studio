# 0018 — the Packs view shows satz's pack graph

- **Status:** accepted
- **Date:** 2026-09-21
- **Deciders:** the maintainer

## Context

The Packs view says which packs make up an estate, what each needs, and switches them.
Under [ADR 0007](0007-pack-rows-are-derived-from-the-estate-file.md) and
[ADR 0015](0015-the-packs-view-draws-the-dependency-tree-the-packs-declare.md), which
this record supersedes, the app worked that out itself: `packs::build` made a row
per `use` line from the file, the questions report and the resolved params, and
`PackDecls::read` parsed every pack file the estate names so `packs::edges` could draw
the tree from the packs' `ask_when`s. A switch was an answer through `satz_interview`,
and the map's own line was uncommented by the app.

That derivation saw only `ask_when`. The packs depend on each other in ways no question
declares: the central alerts, and Sentinel, read params the audit-log archive declares;
the findings mail reads the central alerts'; the billing permissions need a
security-group model; the runner grant reads the runner's. None of those five was
drawn. And the derivation could not tie a `use` line without `when` to the choice that
decides it, so on a real estate a pack showed as having no line and the view offered
`merge-presets`, which changed nothing.

satz ships the answer. The pack graph travels with the presets
(`presets/pack-graph.json`, satz ADR 0031), and one module reads an estate against it
for every reader (satz ADR 0032,
`vendor/satz/docs/adr/0032-one-pack-logic-reads-the-estate-against-the-graph.md`):
`satz_packs` reports every node of the graph — its gate, the estate's answer, the
library's default, where its line stands, whether it deploys, each requirement with
whether it is met, what needs it, what it excludes, its notices and the compile's
findings about it — and `satz_add_pack` and `satz_remove_pack` switch a pack with the
same logic, refusing a switch that would leave a pack on without what it needs. The
three tools are in satz 0.73.0, the `MIN_SATZ` this app already requires.

## Decision

**The Packs view shows satz's pack report and switches through satz's pack tools; the
app derives nothing.** The reload reads `satz_packs` over the estate's session beside
`satz_questions`, and the model carries the `PacksReport` as it came. A switch is
`satz_add_pack` (on) or `satz_remove_pack` (off) under the delegated-write discipline —
the map's line as much as any other, so the app writes no pack line itself. A refusal is
satz's sentence, in a toast and in the drawer. `decls.rs`, `packs::build`,
`packs::edges`, the model's pack notes and the app's pack row types are deleted.

The one thing the view still reads from the file is the phase comment above each `use`
line, by its line number, to put a pack under the phase its line stands under — a
comment satz does not report and that decides nothing.

The tree keeps ADR 0015's drawing — a pack others hang from is a block across the grid,
its dependents below it by right-angle connectors in CSS — over satz's requirements
instead of `ask_when`: a pack hangs below the pack that alone meets its first
requirement. A requirement several packs can meet is the operator's choice between them
and hangs the card nowhere; every requirement that is not met is listed on the card.

ADR 0015 rejected asking satz because the questions report carries only the questions
of packs that are ON, so a tree drawn from it would rearrange whenever a switch flips.
That reason does not hold for the pack report: it has a row for every node of the graph,
on or off, with or without a line in the file, so the tree is the same before and after
a switch.

## Consequences

- Every dependency satz knows is drawn, the five `ask_when` missed among them, and one
  the library gains is drawn on the next pin bump with no edit here.
  `tests/e2e_pack_edges.rs` holds the model's report to `satz packs --format json` for
  the same estate.
- A pack whose line has no `when` is `ungated` in satz's words, with satz's finding that
  says what to write; the view offers no `merge-presets` for it.
- A pack the file has no line for is switched on like any other: `satz_add_pack` writes
  its line where the graph places it.
- A `oneof` is no longer one group card: each option is its own pack, and switching one
  on sets its siblings false, which satz does. The Decisions view still answers the
  choice as a choice.
- A `use` of a file the graph does not know is `unmanaged`: a card with its line and no
  switch, and its gate, where it has one, is a param in the Params view.
- A dependency satz does not know is not shown. The app no longer reads a pack file for
  its dependencies, so a hand-written pack outside the library has none.
- The view needs the MCP session for its rows; a `satz_packs` that fails is a
  diagnostic, and the model is not built, as for a failed `satz_questions`.

## Pros and cons of the options

### A — keep the app's derivation, and read the data edges too

- **Good:** no change to what the view reads; the reload stays offline of satz.
- **Bad:** a second implementation of satz's pack logic in another repository, which
  drifts the day satz's changes; four of the five missing dependencies are satz's `data`
  edges, which the app would have to compute from each pack's params the way satz does,
  and the fifth is a `requires` the pack graph carries; the switch would still be an
  answer that cannot write an absent line.

### B — satz's report for the rows, the app's `ask_when` tree for the edges

- **Good:** the tree keeps the shape the questions give it.
- **Bad:** two sources for one picture that can disagree on the same card; the missing
  dependencies stay missing.

### C — satz's report and satz's switches *(chosen)*

- **Good:** one authority for what the estate uses and what a switch does, the same
  sentences `transpile --check` prints; every pack of the graph, on or off, with or
  without a line; the switch writes absent lines and refuses what would break the
  estate.
- **Bad:** the view says nothing of a dependency outside satz's graph; a requirement
  several packs meet hangs its card under none, so the tree is flatter where the
  operator has a choice to make.
