# 0029 — Components is one destination for presets and packs, and Apply follows the mode

- **Status:** accepted
- **Date:** 2026-11-19
- **Deciders:** the maintainer

## Context

The path through an estate is the same whatever it works in
([ADR 0028](0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md)): what it
is, what it is built from, what those parts ask, its way to the organisation, its
documents. What an estate is built from differs by mode
([ADR 0025](0025-an-estate-works-in-one-of-three-modes-and-satz-says-which.md)): an
organisation's estate from the presets of the library, an estate in a delegated folder
or project from workload packs. Both are optional, and both have dependencies.

The window had two places for them. Presets was a rail destination an estate in a
delegated place did not have; workload packs were a tab of Estate an organisation's
estate did not have ([ADR 0024](0024-workload-packs-are-a-tab-of-estate-and-a-workload-estate-has-no-presets-destination.md)).
So the rail changed with the estate, the second step of the path was missing for one
kind of estate, and its components were under the view that is not a step.

Apply offered every estate an organisation's commands: day 0 and its dry run, and the
state's move to the other backend. Documents offered every estate the compliance
commands, which are made from the catalogs of the preset library — a delegated estate
has none, and `satz require` there answers "no catalog … (available: none)".

## Options

1. Leave the two places and let the next-step bar skip a step. Cost: the rail and the
   walk differ by estate, and packs stay out of the path.
2. **One destination, Components**, whose content follows the mode.
3. Show presets and packs side by side for every estate. Cost: one of the two is empty
   for almost every estate.

## Decision

- **Components is the second step for every estate.** For an organisation's estate it
  is the presets view, unchanged. For an estate in a delegated folder or project it is
  the workload packs list, which leaves the Estate tabs. The rail is the same six
  destinations for every estate.
- **Estate has three tabs:** Params, Resources, Interfaces.
- **Apply follows the mode.** An estate in a delegated place walks compile, rights check,
  generate, prepare, plan, apply; day 0, its dry run and the state's move are the
  organisation's and are named as such, not offered.
- **The compliance commands are offered where the catalogs are**: an estate with a
  preset library. Elsewhere Documents says the organisation's estate reports it.
- **The Overview says what a delegated estate was delegated through**: the interface
  files it reads, beside its mode.
- ADR 0024's tab and its rule that a workload estate has no Presets destination are
  superseded by this; what it says about how the pack commands run stands.

## Consequences

- An estate in a delegated place that also holds a preset library is shown its packs,
  and a sentence that presets are an organisation's view; its presets are switched in
  the file or from the commands palette. No estate known today is one.
- `PacksReport::has_presets` no longer decides a destination; it says whether the
  estate holds the library the compliance catalogs are part of.
- The organisation a delegated estate belongs to is named by the estate's own values
  where it binds them, and by the path of the interface it reads; the app does not open
  that interface for more.
