# 0029 — presets and packs are two destinations, entries are added from their declaration, and a workload estate starts from its interface folders

- **Status:** accepted
- **Date:** 2026-11-19
- **Deciders:** the maintainer

## Context

satz keeps two kinds of pack apart, and its record says why
([ADR 0093](../../vendor/satz/docs/adr/0093-delegation-is-a-base-and-its-targets-and-entries-are-added-by-one-writer.md)):
a **preset** is part of the one library every organisation keeps current — it changes
because organisation policies and the provider change — and a **pack** is a template,
taken once from one of several sources and adapted by the repository that took it. An
organisation's estate uses presets and may use packs; an estate in a delegated folder or
project uses packs.

The window had workload packs as a tab of Estate, shown only to an estate in a delegated
place ([ADR 0024](0024-workload-packs-are-a-tab-of-estate-and-a-workload-estate-has-no-presets-destination.md),
[ADR 0025](0025-an-estate-works-in-one-of-three-modes-and-satz-says-which.md)). So the
path through an estate ([ADR 0028](0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md))
had no step for packs, an organisation could not take one in the window at all, and the
packs of a workload estate stood under the view that is not a step.

Since the same satz release, delegated access is a base and its targets, each an entry
of a list a preset declares with `request`; `satz add-entry` writes such an entry;
`satz init --from` writes a workload estate from the interface folders it was given; and
`satz presets` reports who delegated. Delegating access and starting a workload estate were
edits of a file and a command line the window did not offer.

Apply offered every estate an organisation's commands, and Documents the compliance
commands, which are made from the catalogs of the preset library.

## Options

**Presets and packs in the rail**

1. One destination, "Components", whose content follows what the estate works in.
   Cost: it shows one of two different things under one word, and an organisation, which
   has both, sees only one.
2. **Two destinations: Presets, for an estate that holds the library, and Packs, for
   every estate.** Cost: seven primary destinations for an organisation's estate, the
   limit of the navigation rail.
3. Packs as a tab of Presets. Cost: an estate without presets has no Presets destination
   to hang it under.

**Adding an entry**

1. A form per kind of entry (a delegation, a project, a subnet). Cost: the window learns
   every list, and a new preset needs a new form.
2. **One form built from the declaration** satz reports: a field per declared field, its
   pattern, its default.

## Decision

- **Presets and Packs are two primary destinations.** Presets is what it was, for an
  estate that holds the preset library. Packs is the workload packs list for every
  estate, worded as what a pack is: a template to take and adapt. The path reads
  Overview, Presets, Packs, Decisions, Apply, Documents; an estate without presets goes
  from the Overview to Packs. Estate has three tabs.
- **An entry is added from its declaration.** Estate → Interfaces lists every list the
  estate's packs declare with the entries it holds, and "Add" opens a form of the
  declared fields — a switch for a yes or no, a text field for the rest, with the pattern
  under it and the default as the value an untouched field keeps. The entry is written by
  satz (`satz_add_entry`) under the delegated-write discipline; what was left as its
  default is not sent. Each published interface has "Show folder", which opens what the
  last transpile wrote for it under `interfaces/outgoing/`.
- **A workload estate starts from its interface folders.** The Create door has a second
  form, "A team, from interface folders": the folders, an optional name, and the same
  target folder; it runs `satz init --from`.
- **The Overview says who delegated**: one sentence per interface file the estate reads,
  from `delegated_by` of satz's presets report.
- **Apply follows what the estate works in.** An estate in a delegated place walks
  compile, rights check, generate, prepare, plan, apply; day 0, its dry run and the
  state's move are the organisation's and are named, not offered.
- **The compliance commands are offered where the catalogs are**: an estate that holds
  the preset library.
- `MIN_SATZ` is 0.101.0: the tool, the two report fields and `init --from` are that
  release's.
- ADR 0024's tab, and ADR 0025's rule that an organisation has no workload packs, are
  superseded by this.

## Consequences

- The rail is full for an organisation's estate: seven, with Estate after the path. A
  further view is a tab.
- The form adds an entry; changing or removing one is an edit of the estate file, as
  with `satz add-entry`.
- The window does not copy an interface folder to a team. "Show folder" opens it; the
  hand-over is the operator's.
- An estate reads one delegated place today, which is satz's rule; the Create form
  takes any number of folders and satz refuses more than one place.
