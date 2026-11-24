# 0037 — Estate is a step of the path, before Apply

- **Status:** accepted
- **Date:** 2026-11-24
- **Deciders:** the maintainer

## Context

ADR 0028 made the primary destinations a path with a bar at the window's foot — the step
before, the step after — and left Estate out of it, last in the sidebar after Documents:
the estate file itself, for whoever wants a value the questions do not ask for, was not
taken to be a step.

Estate is where such a value is set — a param no pack asks about, a resource's attribute
— and that has to happen before the transpile on Apply writes the HCL code. Standing after
Documents, it read as an afterthought, and the path went from the questions straight to
the transpile.

## Options

1. **Estate between Decisions and Apply in the sidebar, off the path bar.** The order
   says where it belongs; the walk from Decisions still goes straight to Apply. Costs a
   sidebar whose order and walk differ in the middle.
2. **Estate between Decisions and Apply, and a step of the path.** The sidebar and the
   walk are one order; everyone walking the path passes Estate, whether the estate needs
   a value there or not.
3. **Leave it last, off the path.** No change; Estate stays after the documents.

## Decision

Option 2, the maintainer's choice. The sidebar reads Overview, Presets, Packs, Decisions,
Estate, Apply, Security, Documents, and the path bar walks the same order: its steps are
the primary destinations (`STEPS` is `View::PRIMARY`). Estate's purpose on the bar is
"Set any value of the estate file the questions do not ask for."

## Consequences

- Decisions leads on to Estate, and Estate to Apply.
- The sidebar and the path bar cannot drift apart: the path is the sidebar's list.
- ADR 0028's "Estate stands after them and is not a step" no longer holds; the rest of
  0028 does.
