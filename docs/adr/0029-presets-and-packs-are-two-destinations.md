# 0029 — presets and packs are two destinations, entries are added from their declaration, and a workload estate starts from its interface folders

- **Status:** accepted
- **Date:** 2026-11-19
- **Deciders:** the maintainer

## Context

satz keeps two kinds of pack apart, and its record says why
([ADR 0093](../../vendor/satz/docs/adr/0093-delegation-is-a-base-and-its-targets-and-entries-are-added-by-one-writer.md)):
a **preset** is part of the one library every organization keeps current — it changes
because organization policies and the provider change — and a **workload pack** is a template,
taken once from one of several sources and adapted by the repository that took it. An
organization's estate uses presets and may use workload packs; a workload estate uses
workload packs.

The window had workload packs as a tab of Estate, shown only to a workload
estate ([ADR 0024](0024-workload-packs-are-a-tab-of-estate-and-a-workload-estate-has-no-presets-destination.md),
[ADR 0025](0025-an-estate-works-in-one-of-three-modes-and-satz-says-which.md)). So the
path through an estate ([ADR 0028](0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md))
had no step for packs, an organization could not take one in the window at all, and the
packs of a workload estate stood under the view that is not a step.

Since the same satz release, a delegation and its delegated resources are each an entry
of a list param a preset declares with `entries`; `satz add-entry` writes such an entry;
`satz init --from` writes a workload estate from the interface folders it was given; and
`satz presets` reports who delegated. Delegating access and starting a workload estate were
edits of a file and a command line the window did not offer.

Apply offered every estate an organization's commands, and Documents the compliance
commands, which are made from the catalogs of the preset library.

## Options

**Presets and packs in the rail**

1. One destination for presets and workload packs together, whose content follows which
   kind of estate it is.
   Cost: it shows one of two different things under one word, and an organization, which
   has both, sees only one.
2. **Two destinations: Presets, for an estate that holds the library, and Packs, for
   every estate.** Cost: seven primary destinations for an organization's estate, the
   limit of the navigation rail.
3. Packs as a tab of Presets. Cost: an estate without presets has no Presets destination
   to hang it under.

**Adding an entry**

1. A form per kind of entry (a delegation, a delegated project, a subnet). Cost: the window
   learns every list param, and a new preset needs a new form.
2. **One form built from the declaration** satz reports: a field per declared field, its
   pattern, its default.

## Decision

- **Presets and Packs are two primary destinations.** Presets is what it was, for an
  estate that holds the preset library. Packs is the workload packs list for every
  estate, worded as what a pack is: a template to take and adapt. The path reads
  Overview, Presets, Packs, Decisions, Apply, Documents; an estate without presets goes
  from the Overview to Packs. Estate has three tabs.
- **An entry is added from its declaration.** Estate → Interfaces lists every list param the
  estate's packs declare with the entries it holds, and "Add" opens a form of the
  declared fields — a switch for a yes or no, a text field for the rest, with the pattern
  under it and the default as the value an untouched field keeps. The entry is written by
  satz (`satz_add_entry`) under the delegated-write discipline; what was left as its
  default is not sent. Each published interface has "Show folder", which opens what the
  last transpile wrote for it under `interfaces/outgoing/`.
- **A workload estate starts from its interface folders.** The Create door has a second
  form, "A team, from interface folders": the folders, an optional name, and the same
  folder on disk to create it in; it runs `satz init --from`.
- **The Overview says who delegated**: one sentence per interface file the estate reads,
  from `delegated_by` of satz's presets report.
- **Apply follows which kind of estate it is.** A workload estate walks
  `transpile --check`, `update-prerequisites`, `transpile`, `hcl-init`, `plan`, `apply`;
  the bootstrap, its dry run and the state's move are the organization's and are named, not offered.
- **The compliance commands are offered where the catalogs are**: an estate that holds
  the preset library.
- `MIN_SATZ` is 0.102.0: the tool, the report fields and `init --from` are that
  release's or older, and the delegation's fields (`estate_account`,
  `estate_account_user_group`, `infrastructure_project`) and `list_params` are 0.102.0's.
- ADR 0024's tab, and ADR 0025's rule that an organization has no workload packs, are
  superseded by this.

## Consequences

- The rail is full for an organization's estate: seven, with Estate after the path. A
  further view is a tab.
- The form adds an entry; changing or removing one is an edit of the estate file, as
  with `satz add-entry`.
- The window does not copy an interface folder to a team. "Show folder" opens it; copying
  it is the operator's.
- A workload estate reads the interface folders of one delegation, which is satz's rule;
  the Create form takes any number of folders and satz refuses more than one delegation.
