# 0025 — an estate works in one of three modes, and satz says which

- **Status:** accepted
- **Date:** 2026-11-12
- **Deciders:** the maintainer

## Context

[ADR 0024](0024-workload-packs-are-a-tab-of-estate-and-a-workload-estate-has-no-presets-destination.md)
knew two kinds of estate: one with a preset library, and a "workload estate", which the
app recognised itself — the presets report named an interface file and held no pack.

satz 0.99.0 names three places an estate works in, and reports them:

- its presets report carries `works_in`: `organisation`, `folder` (an interface the estate
  uses publishes a delegated folder) or `project` (it uses an interface and none does);
- `satz packs` carries, per workload pack, `needs`: `folder`, `project`, both or nothing,
  derived from the delegated values the pack reads.

A workload pack that creates a project needs a folder to put it in; one that fills a
project needs that project. An estate in a delegated project has no folder to give, and
the window listed it every pack of the source all the same.

## Options

**Who decides the mode**

1. The app, from the interface files, as it did for the two-way rule. Cost: a second
   reading of what satz already reads, which drifts with every export satz renames.
2. satz's report. Cost: the app needs satz 0.99.0.

**What an estate is shown of packs it cannot use**

1. Every pack, the unusable ones marked. Cost: a list whose rows are mostly not for this
   estate, each with a Fetch that leads to a refusal at the compile.
2. Only what the mode has a use for, and one line that counts the rest.

**Where the mode is said**

1. A chip in the top bar. Cost: the bar carries the estate's address and its commands and
   nothing else (ADR 0014, amended: the top bar carries no chips).
2. A row of the Overview's identity card, "Works in", with an assist chip.

## Decision

- **The mode is satz's word.** `PacksReport::works_in` is `EstateMode { Organisation,
  Folder, Project }`, a required field: a report without it is an older satz's and is
  refused, never guessed. `PacksReport::workload_estate` is gone.
- **Organisation:** the Presets destination, as before; Estate has three tabs — an
  organisation's optional components are the library's presets, and the Workload packs tab
  is not shown.
- **Delegated folder:** no Presets destination unless the estate holds a library of its
  own; the Workload packs tab offers every pack of the source — an estate in a folder can
  create a project there.
- **Delegated project:** as the folder, and the tab offers the packs that need a project
  or nothing. Packs that need a folder are counted in one line, not listed.
- **A pack's row says what it needs** in an assist chip, "needs a delegated project".
- **The mode is a row of the Overview's identity card**, with a sentence that says what it
  means for the estate. The Overview's day-0 row (`bootstrap`) is an organisation's alone.
- ADR 0024 stands in everything else; its two-way rule is superseded by this one.

## Consequences

- `MIN_SATZ` is 0.99.0.
- The rail shows six primary destinations for an organisation's estate and five for an
  estate in a delegated place without a library.
- A pack that needs a scope it does not read is offered where it cannot work. satz derives
  the need and declares none; the remedy is satz's.
- An organisation's estate that wants a workload pack fetches it from the commands
  palette; the window gives it no tab.
