# 0015 — the Packs view draws the dependency tree the packs declare

- **Status:** accepted
- **Date:** 2026-09-16
- **Deciders:** the maintainer

## Context

The Packs view explains which packs make up an estate: one card per pack line, in
sections by the phase comment above the line, each section a responsive grid
(`repeat(auto-fill, minmax(420px, 1fr))`). What it did not show is that some packs are
only asked for once another is on. satz declares that dependency on the question:
`ask_when = <param>` asks it only while that param is truthy (satz ADR 0006,
`vendor/satz/docs/adr/0006-an-answer-is-a-param-the-estate-binds.md`), and the map
declares its choices as questions (satz ADR 0007). Today the map waits Security Command
Center's notifications and export on its enablement, the mail and the SIEM subscription
on the notifications, and Sentinel's two log paths on Sentinel, which they also default
to by reference; the Defender pack waits its CSPM access `oneof` on the CSPM plan. The
dependents' lines stand under later phases than their parents, so in the flat view a
pack and the packs that need it sat sections apart.

Two questions: where the edges come from, and how they are drawn.

## Decision

**The edges are the packs' own `ask_when`s.** `PackDecls::read` parses the estate file
and every file its `use` lines reach — active or commented, whatever the gate — with
`satz_core::satz::parse` through the loader satz's pipeline uses, and keeps each
question's `ask_when` and whether the param's applying binding is that gate by
reference. An `ask_when` between two pack rows (the row keyed by its gate, a `oneof` by
its options) is a `PackEdge`. A question waits on at most one gate, so the edges are a
forest; a declaration that would break that, and a file that cannot be read, is a note
and no edge. A dependency stated only in a question's `why` is not read.

**A pack others wait on is a block across the grid; the connectors are CSS.** A card
with no dependents and no parent stays a grid cell. A card with dependents becomes one
item spanning the grid row (`grid-column: 1 / -1`, the mechanism the `oneof` group card
already used): the card, and below it its dependents in the file's order, each joined by
a right-angle connector — a vertical trunk and a horizontal branch — recursing for a
dependent that has dependents. A dependent hangs under its parent even when its line
stands under another phase, and carries that phase as a caption. The connectors are
borders on the tree's own nested elements (`ConnectorTree`, `.m-connector-tree`).

## Consequences

- A dependency the library gains is drawn on the next pin bump without an edit here, and
  `tests/e2e_pack_edges.rs` holds the edges to an independent parse of the same files.
- Packs that are off are read, so the tree is the same before and after a switch flips;
  a reload reads every pack a line names, about forty small files for a skeleton.
- The grid keeps its density: only the few packs others wait on leave it.
- A dependent is not listed under its own phase; its caption says which phase it is, and
  a section left empty is not shown.
- A connector meets its card at a fixed offset (the centre of a card's head row). A head
  that wraps at a narrow width meets the connector above its centre; nothing detaches.
- The "follows" mark and the error connector are derived from the same rows the cards
  show: follows where the binding that applies is the parent by reference, the error
  colour where the child's pack is in the estate (line on, gate on) while its parent's is
  not.

## Pros and cons of the options

### Where the edges come from

#### A — a table of edges in the app

- **Good:** no parsing; the edges exist even for a file that cannot be read.
- **Bad:** it is the copied table ADR 0007 refused for the rows, and it drifts the day
  the library gains or drops a dependency, with no test in this repository to notice.

#### B — ask satz: extend the questions report with `ask_when`

- **Good:** one authority; no second read of the pack files.
- **Bad:** needs a satz release first, and the report carries only the questions of packs
  that are ON, so the tree would rearrange whenever a switch flips; the by-reference
  default is not in the report either.

#### C — parse the packs with satz-core *(chosen)*

- **Good:** satz-core is already a dependency and the authority on meaning (ADR 0003);
  the files are the record; commented packs are readable; the reference default is
  visible in the parse.
- **Bad:** a second read of files the params fold has read; the app must say what it
  does with a file that does not parse, which is a note.

### How they are drawn

#### D — lines over today's grid, in SVG measured from the cards

- **Good:** every card stays where it is.
- **Bad:** in an `auto-fill` grid a line between two cards crosses the cards between them
  and moves with every resize; it needs measuring after layout and re-measuring on
  resize and zoom, and a dependent sections away gives a line across the page.

#### E — a separate graph view

- **Good:** room for any layout.
- **Bad:** a second place for the same cards and switches, and the phase order the view
  is read in is lost.

#### F — tree blocks in the grid, connectors in CSS *(chosen)*

- **Good:** a tree cannot cross itself by construction; borders on nested elements need
  no measuring and survive resize and zoom; the cards are the same `PackCard`s; the
  grid is untouched for the packs that stand alone.
- **Bad:** a dependent leaves its phase section; the connector's meeting point is a fixed
  offset rather than the measured centre of the card's head.
