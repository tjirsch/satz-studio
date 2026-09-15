# 0011 — the licence is Apache 2.0

- **Status:** accepted
- **Date:** 2026-09-15
- **Deciders:** the maintainer

## Context

The repository is public and carries a licence, which is the first file a reader of a
public repository looks at and the first thing a legal review reads. This app is
operated inside enterprises: it is installed on a workstation that reaches a customer's
cloud organisation, so the people who decide whether it may be installed are the ones
who read the licence.

The MIT Licence the repository opened with is a copyright grant and a warranty
disclaimer. It is silent on patents — whether a contributor may assert a patent against
someone using the work is left to an implied licence that has to be argued rather than
read — and silent on what terms a contribution arrives under, which is the question a
project answers either in its licence or with a separate agreement to sign.

satz is the tool this app drives, pinned once as the submodule `vendor/satz`, and
`satz-core` is a path dependency into that checkout: a built bundle is a combined work.
satz is taking the Apache License 2.0 in the same act. An app and the tool inside it
that disagree about their terms is a question a reviewer has to resolve before either
can be installed, and neither answer would be useful to them.

The cost of deciding this is at its lowest right now. Every commit in the history is
authored and committed by the maintainer, both crates are `publish = false` and nothing
has gone to crates.io, and there are no outside contributors and no forks. The sole
copyright holder can change the terms; after the first contribution from anyone else,
the same change needs that person's agreement.

## Decision

The repository is licensed under the Apache License 2.0.

- `LICENSE` is the licence text verbatim, appendix included.
- `license = "Apache-2.0"` under `[workspace.package]` in the workspace `Cargo.toml`;
  both crates keep `license.workspace = true` and neither overrides it.
- `NOTICE` at the root is what §4(d) makes a redistributor carry: the project, the
  copyright line, the standard "Licensed under the Apache License, Version 2.0"
  paragraph, and every piece of material bundled here under other terms, each with its
  licence and where that licence text sits — the Material Symbols Rounded font
  (`crates/satz-studio/assets/fonts/`, Apache-2.0), the vendored tree-sitter grammar for
  Satz (`vendor/satz-tree-sitter/`, MIT, a separate repository), tree-sitter's runtime
  headers emitted beside the generated parser (MIT), and satz itself.
- `CONTRIBUTING.md` states what §5 of the licence already says: a contribution submitted
  for inclusion is licensed under these same terms. There is no contributor licence
  agreement and none is asked for.
- Source files carry no licence header. The appendix suggests one per file; the licence
  is in `LICENSE` and in the manifests, and a header on every file is noise that goes
  stale.

## Consequences

- The `NOTICE` travels with a redistribution, and it is only correct if it is kept
  correct: a change that bundles a file from elsewhere adds its line in the same pull
  request. That rule is in `CLAUDE.md` and `CONTRIBUTING.md`; no check enforces it.
- The bundles `dx bundle` builds and `release.yml` attaches carry neither `LICENSE` nor
  `NOTICE` today — only the bundle and its SHA-256 sidecar. Putting both inside the
  bundles, or showing the notice in the window, is open work.
- The patent grant in §3 is the reason for the change, and it comes with the
  retaliation clause attached to it: anyone who brings a patent claim over this work
  loses the grant. That is the intended effect, not a side effect.
- Apache 2.0 is incompatible with GPLv2 (it is compatible with GPLv3). A downstream that
  must ship under GPLv2 can no longer take this code. Nothing here is aimed at such a
  downstream, and the app is a desktop binary rather than a library.
- The vendored grammar stays MIT. It is a separate repository and its terms are its own
  decision; MIT material is redistributable inside an Apache 2.0 work, which is what the
  `NOTICE` records.
- What was already published under MIT stays available under MIT to whoever has it: a
  licence once granted is not withdrawn. The change binds what is distributed from here
  on.
- Changing the licence again is another act of relicensing, and it stays this cheap only
  while there is one copyright holder.

## Pros and cons of the options

### A — the Apache License 2.0 alone *(chosen)*

- **Good:** an express patent grant and a retaliation clause, which is the first
  question asked of a tool that touches a customer's cloud estate; contribution terms
  stated in §5, so no separate agreement is needed from anyone; the licence corporate
  review passes without discussion; the same terms as the tool this app drives, so the
  combined work has one answer instead of two.
- **Bad:** 202 lines against MIT's 21, and a `NOTICE` file that has to be maintained
  by hand; incompatible with GPLv2; the change has to be made now or not cheaply at all.

### B — stay MIT

- **Good:** nothing to do; the shortest licence anyone actually reads; compatible with
  everything, GPLv2 included.
- **Bad:** says nothing about patents, so the reviewer's first question has no answer in
  the text; says nothing about the terms a contribution arrives under, which is what a
  contributor licence agreement otherwise exists to settle; and it would disagree with
  satz, which the app links and ships.

### C — `MIT OR Apache-2.0`, the Rust ecosystem's usual dual licence

- **Good:** the convention a Rust reader expects, and the widest compatibility of the
  three — a downstream needing GPLv2 takes the MIT arm.
- **Bad:** the patent grant becomes optional, since a downstream may take the MIT arm
  and leave it; the convention exists for libraries published on crates.io, and neither
  crate is published; two licence files, and a `NOTICE` whose obligations depend on
  which arm the redistributor chose.
